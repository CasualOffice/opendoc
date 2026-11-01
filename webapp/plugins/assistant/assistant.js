// SPDX-License-Identifier: Apache-2.0
// Casual AI's panel: the assistant as a first-party plugin on the public plugin
// API (`docs/183` §13, §21, §23, owner decision 3). It reads with the read tools,
// asks the configured model with `ai/complete` (never seeing the key), and makes
// every change as a LIVE CO-EDITOR (`editor.edit`, §23): the editor writes the
// text at a visible "Casual AI" pointer, batched, as one undo step, within the
// effective access, and the card shows "Casual AI is writing… · Stop · Accept all
// · Reject all". The private preview remains for other add-ins and for replies.
//
// THE PANEL (owner review, 2026-10-31: "too crowded, redundant information, too
// narrow, we need something tangible that basically works"). Designed from
// Gemini's side panel in Google Docs and Copilot's in Word:
//
//   * the conversation fills the body, with suggestions while it is empty;
//   * ONE composer at the bottom: a growing text field, Send that becomes Stop
//     while the model writes, a chip naming the selection only while there is
//     one, and an ACCESS BADGE (Editing / Suggesting / Commenting / Read-only)
//     that says what the AI may do here;
//   * every answer is a CARD with one clear primary action that changes the
//     document — Insert at the cursor, Replace selection, Add as comment, Apply —
//     or, where the access admits no change, the allowed alternative (a comment,
//     Copy) with the reason said in words;
//   * what is not the conversation is behind the editor's ⋯ in the header line.
//
// Chat answers stream into their cards; EDITS stream into the document (owner,
// 2026-10-31, superseding rule 4's "never as jumps in the document": the
// pointer makes the AI's work visible as a person's would be).
(function () {
  "use strict";
  var Core = window.CasualAssistantCore;
  var $ = function (id) { return document.getElementById(id); };
  var NS = "plugins.assistant.";
  var KEYS = [
    "disclosure.endpoint", "disclosure.host", "logLabel", "empty", "you", "ai", "prompt.label", "prompt.placeholder",
    "prompt.draftPlaceholder", "prompt.tablePlaceholder", "send", "stop", "cleared", "chip.summary", "chip.keyPoints", "chip.draft", "chip.review",
    "chip.table", "chip.describeTable", "chip.turnIntoTable",
    "working", "reading", "stopped", "dropped", "citation", "insert", "replace", "comment", "apply", "applyDirect", "applyComments", "copy",
    "copied", "truncated", "noSelection", "noCaret", "noTable", "unchanged", "unparsable", "unparsableFormat", "skipped", "workingLocal", "truncatedLocal", "live.empty", "violation.stale", "violation.empty", "violation.incomplete", "translateTo", "translateGo", "noComments",
    "draftReply", "historyNote", "error.generic", "error.auth", "error.rate", "error.network", "error.timeout",
    "error.budget", "error.tooLong", "error.refused", "replyFor", "caveat", "workingOn", "retry", "attached", "detach",
    "draftMode", "draftCancel", "tableMode", "reviewSummary", "proposed", "diff.inserted", "diff.deleted",
    "live.writing", "live.done", "live.stopped", "live.acceptAll", "live.rejectAll", "live.keep", "live.undoAll",
    "live.accepted", "live.rejected", "live.rolledBack", "live.refused", "live.stale",
    "violation.injection", "violation.plan", "violation.control", "violation.length", "violation.access", "violation.engine",
    "access.label", "access.cannotEdit", "access.cannotComment", "replaceTable", "replaceTableConfirm", "describeTable",
  ].concat(
    ["ask", "rewrite", "shorten", "expand", "grammar", "formal", "friendly", "confident", "translate", "summarise", "review", "draft", "replies", "table", "describeTable"].map(function (id) { return "command." + id; }),
  ).map(function (k) { return NS + k; });
  var LANGUAGES = ["ar", "de", "en", "es", "fr", "hi", "id", "it", "ja", "ko", "nl", "pl", "pt", "ru", "tr", "uk", "vi", "zh"];
  var READ_CAP = 60000;
  /** What a model on this computer is assumed to hold when nothing says more:
   *  Ollama's default context window is 4,096 tokens, and its OpenAI-compatible
   *  endpoint truncates a longer prompt FROM THE START (the system prompt goes
   *  first) without an error. Sending what fits, and saying so, beats that. */
  var LOCAL_CONTEXT = 4096;
  /** A known answer replayed into the document: this many characters per piece,
   *  one piece per tick, so the pointer is seen writing it (the editor batches). */
  var REPLAY_CHARS = 12;
  var REPLAY_MS = 40;
  /** The storage key that remembers the first-run note was shown. Not a
   *  document key (those are 8 hex digits), so eviction never takes it. */
  var SEEN = "~seen";

  var editor = null;
  var info = null;
  var strings = {};
  var history = [];
  var running = null;
  /** The selection the composer's chip names: `{text, anchor, focus, table}`, or null. */
  var attached = null;
  /** The table the caret is in, when it is in one. */
  var caretTable = null;
  var mode = "ask";
  var seenNote = true;
  /** What the AI may do here (`edit/access`), refreshed on `access/changed`. */
  var access = { mode: "chat", badge: "readonly", capability: "", badgeText: "", reasonText: "" };
  /** Live turns by id: `{out, live}`. */
  var turns = Object.create(null);
  /** Subscriptions made once the editor has connected. */
  var editorEvents = [];
  var VIOLATIONS = ["injection", "plan", "control", "length", "access", "engine", "stale", "empty", "incomplete"];

  function s(key, params) {
    var text = strings[NS + key] || "";
    if (params) Object.keys(params).forEach(function (name) { text = text.split("{" + name + "}").join(String(params[name])); });
    return text;
  }
  var clip = function (text, max) { return text.length > max ? text.slice(0, max - 1) + "…" : text; };
  var encoder = new TextEncoder();
  var bytes = function (text) { return encoder.encode(String(text || "")).length; };
  var canEdit = function () { return access.mode === "direct" || access.mode === "suggest"; };
  var canComment = function () { return canEdit() || access.mode === "comment"; };

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
   *  note (what happened), live (the turn control), actions. */
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
    var live = document.createElement("div");
    live.className = "ca-live";
    live.hidden = true;
    var actions = document.createElement("div");
    actions.className = "ca-card-actions";
    actions.hidden = true;
    root.append(body, extra, note, live, actions);
    $("log").append(root);
    scrollDown();
    return { root: root, body: body, extra: extra, note: note, live: live, actions: actions };
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

  // ---- The access badge (`docs/183` §23.3) ------------------------------------

  function reflectAccess(next) {
    if (next && typeof next === "object") access = next;
    var badge = $("accessBadge");
    if (!badge) return;
    badge.dataset.access = access.badge || "readonly";
    $("accessBadgeText").textContent = access.badgeText || "";
    var icon = access.badge === "editing" ? "edit" : access.badge === "suggesting" ? "rate_review" : access.badge === "commenting" ? "add_comment" : "visibility";
    badge.firstElementChild.textContent = icon;
    var why = access.reasonText ? " — " + access.reasonText : "";
    badge.title = s("access.label", { mode: access.badgeText || "" }) + why;
    badge.setAttribute("aria-label", badge.title);
    badge.hidden = !access.badgeText;
  }

  function refreshAccess() {
    return editor.edit.access().then(reflectAccess).catch(function () {});
  }

  /** A request with the capability line the editor gave (fixed English). */
  function told(body) {
    return Core.withAccess(body, access.capability);
  }

  // ---- Model calls ------------------------------------------------------------

  /** One streamed completion. `target` (a card) shows the text as it comes, or
   *  `onDelta` receives each delta (a live turn writes it into the document).
   *  Resolves with the full text. */
  function complete(body, target, workingKey, onDelta) {
    var controller = new AbortController();
    running = controller;
    // A model on this computer may think for minutes before its first word (a
    // CPU-only server evaluating the prompt): say so until the first word comes.
    var local = !!(info && info.provider && info.provider.local);
    var waiting = local;
    setWorking(s(local ? "workingLocal" : workingKey || "working"));
    var text = "";
    if (target) target.body.classList.add("ca-pending");
    return editor.ai
      .complete(told(body), {
        signal: controller.signal,
        onChunk: function (chunk) {
          var delta = chunk && chunk.choices && chunk.choices[0] && chunk.choices[0].delta;
          if (delta && typeof delta.content === "string") {
            if (waiting && delta.content) {
              waiting = false;
              setWorking(s(workingKey || "working"));
            }
            text += delta.content;
            if (onDelta) onDelta(delta.content);
            else if (target) {
              // A chat card shows the answer without the prompt's data markers.
              target.body.textContent = Core.scrubMarkers(text);
              scrollDown();
            }
          }
        },
      })
      .then(function (result) {
        var message = result.response.choices[0].message;
        var full = typeof message.content === "string" ? message.content : text;
        // A server that did not stream answers once: the whole text is one delta.
        if (onDelta && full && full.length > text.length && full.slice(0, text.length) === text) onDelta(full.slice(text.length));
        // A card's answer (and what Insert or Add as comment takes from it) is
        // scrubbed; a live edit's is not, so its guard still sees a marker.
        return target ? Core.scrubMarkers(full) : full;
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

  // ---- Live co-editing (`docs/183` §23) ----------------------------------------

  /** The card's compact control while a turn runs and until it is decided:
   *  "Casual AI is writing… · Stop · Accept all · Reject all". */
  function renderLive(out, live) {
    var box = out.live;
    box.replaceChildren();
    var status = document.createElement("span");
    status.className = "ca-live-status";
    status.setAttribute("role", "status");
    var dot = document.createElement("span");
    dot.className = "ca-live-dot";
    dot.setAttribute("aria-hidden", "true");
    status.append(dot, document.createTextNode(
      live.state === "writing" ? s("live.writing")
        : live.state === "stopped" ? s("live.stopped")
          : live.state === "accepted" ? s("live.accepted")
            : live.state === "rejected" ? s("live.rejected")
              : live.state === "rolledBack" && live.code === "empty" ? s("live.empty")
                : live.state === "rolledBack" ? s("live.rolledBack", { reason: s("violation." + (live.code || "engine")) })
                : s("live.done"),
    ));
    box.dataset.state = live.state;
    box.append(status);
    var open = live.state === "writing" || live.state === "done" || live.state === "stopped";
    if (live.state === "writing") {
      var stop = button(s("stop"), "btn ca-live-stop");
      stop.dataset.live = "stop";
      stop.addEventListener("click", function () { stopTurn(live); });
      box.append(stop);
    }
    if (open) {
      var direct = live.mode === "direct" || live.kind === "comment";
      var accept = button(s(direct ? "live.keep" : "live.acceptAll"), "btn btn-primary");
      accept.dataset.live = "accept";
      accept.addEventListener("click", function () { decideTurn(live, true); });
      var reject = button(s(direct ? "live.undoAll" : "live.rejectAll"), "btn");
      reject.dataset.live = "reject";
      reject.addEventListener("click", function () { decideTurn(live, false); });
      box.append(accept, reject);
    }
    box.hidden = false;
    scrollDown();
  }

  function setLive(live, state, extra) {
    live.state = state;
    if (extra && extra.code) live.code = extra.code;
    var held = turns[live.turn];
    if (held) renderLive(held.out, live);
  }

  function stopTurn(live) {
    if (live.state !== "writing") return;
    if (running) running.abort();
    live.stopping = true;
    void editor.edit.stop(live.turn).catch(function () {});
    setLive(live, "stopped");
  }

  function decideTurn(live, accept) {
    if (running && live.state === "writing") running.abort();
    live.stopping = true;
    return editor.edit.decide(live.turn, accept).then(function (answer) {
      if (answer && answer.ok === false) return;
      setLive(live, accept ? "accepted" : "rejected");
    }).catch(function () {});
  }

  /** Opens a live turn for `out`. Resolves with the turn, or null when the
   *  editor refused it (the card says why, and offers what is allowed). */
  function beginTurn(out, spec, kind) {
    return editor.edit.begin(spec, kind || "text").then(function (opened) {
      if (!opened || opened.ok === false) {
        out.note.textContent = opened && opened.code === "stale" ? s("live.stale") : s("live.refused", { reason: (opened && opened.message) || "" });
        return null;
      }
      var live = { turn: opened.turn, mode: opened.mode, cells: opened.cells || null, kind: kind || "text", state: "writing", code: null, stopping: false };
      turns[live.turn] = { out: out, live: live };
      renderLive(out, live);
      return live;
    });
  }

  /** A guard for the turn: the core's incremental validator. */
  function guardFor(live, action, selected) {
    live.guard = Core.createGuard({ mode: live.mode, budget: Core.budgetFor(action, selected) });
    return live.guard;
  }

  /** Writes `piece` into the turn, or aborts it on a violation. False when the
   *  turn must not continue. */
  /** How often validated text leaves the panel for the editor. The port admits
   *  20 requests a second (`plugin_rpc.mjs` RPC_BOUNDS), so a fast model's deltas
   *  are coalesced here and the editor batches again on its side. */
  var SEND_MS = 100;

  function post(live, piece, cell) {
    void editor.edit.write(live.turn, piece, cell).then(function (answer) {
      if (answer && answer.ok === false && !live.code && VIOLATIONS.indexOf(answer.code) >= 0) {
        live.code = answer.code || "engine";
        if (running) running.abort();
        setLive(live, "rolledBack", { code: live.code });
      }
    }).catch(function () {});
  }

  /** Sends what the panel has buffered for the turn, now. */
  function drainOut(live) {
    if (live.outTimer) clearTimeout(live.outTimer);
    live.outTimer = 0;
    if (live.out && !live.code && !live.stopping) post(live, live.out, null);
    live.out = "";
  }

  /** Validates `piece` and queues it for the turn, or aborts the turn on a
   *  violation. False when the turn must not continue. A `cell` move is sent at
   *  once, after whatever was queued for the cell before it. */
  function send(live, guard, piece, cell, final) {
    if (live.code || live.stopping) return false;
    // What could still become a data marker ("<", "</doc", …) is held until the
    // next delta decides it, so a marker never reaches the page even briefly.
    piece = (live.hold || "") + (piece || "");
    live.hold = "";
    if (!final) {
      var lt = piece.lastIndexOf("<");
      if (lt >= 0) {
        var tail = piece.slice(lt);
        var name = tail.replace(/^<\s*\/?\s*/, "");
        if (tail.length < 24 && /^<\s*\/?\s*[a-z_]*$/i.test(tail) && "document_text".indexOf(name.toLowerCase()) === 0) {
          live.hold = tail;
          piece = piece.slice(0, lt);
        }
      }
    }
    if (!piece) return true;
    var code = guard.check(piece);
    if (code) {
      live.code = code;
      live.out = "";
      if (running) running.abort();
      void editor.edit.abort(live.turn, code).catch(function () {});
      setLive(live, "rolledBack", { code: code });
      return false;
    }
    if (cell) {
      drainOut(live);
      post(live, piece, cell);
      return true;
    }
    live.out = (live.out || "") + piece;
    if (!live.outTimer) live.outTimer = setTimeout(function () { drainOut(live); }, SEND_MS);
    return true;
  }

  function endTurn(live) {
    if (live.hold && live.guard) send(live, live.guard, "", null, true);
    if (live.code || live.stopping) return Promise.resolve();
    drainOut(live);
    return editor.edit.end(live.turn).then(function (answer) {
      if (answer && answer.ok === false) {
        // The editor reverted a turn that wrote nothing ("empty") or found the
        // text changed under it ("stale"): the card says so, nothing to accept.
        if (VIOLATIONS.indexOf(answer.code) >= 0 && !live.code) {
          live.code = answer.code;
          setLive(live, "rolledBack", { code: answer.code });
        }
        return;
      }
      if (live.state === "writing") setLive(live, "done");
    }).catch(function () {});
  }

  /** Streams a model answer INTO the turn, shaped and guarded per delta, and
   *  ends the turn. */
  function streamInto(live, body, options) {
    return streamPart(live, body, options).then(function (full) {
      if (full === null) return null;
      return endTurn(live).then(function () { return full; });
    });
  }

  /** Streams one model answer into the open turn (one part of a rewrite across
   *  paragraphs, or the whole of any other), shaped and guarded per delta, and
   *  leaves the turn open. Resolves with the answer, or null when stopped. */
  function streamPart(live, body, options) {
    var o = options || {};
    var shaper = Core.createShaper({ paragraphs: o.paragraphs !== false });
    var guard = guardFor(live, o.action, o.selected);
    return complete(body, null, "working", function (delta) {
      send(live, guard, shaper.push(delta));
    }).then(function (full) {
      send(live, guard, shaper.finish(), null, true);
      return full;
    }, function (err) {
      var cancelled = !err || err.code === "cancelled";
      if (!live.code && !live.stopping) {
        if (cancelled) {
          // Stopped by the person (the panel's Stop): what was written stays,
          // and a turn that had written nothing yet is reverted by the editor.
          void editor.edit.stop(live.turn).catch(function () {});
          setLive(live, "stopped");
        } else {
          // The model failed (a first-byte timeout, a dropped connection, a
          // provider error): an unfinished rewrite is not what was asked for,
          // so the whole turn is rolled back and the original text stays.
          live.code = "incomplete";
          void editor.edit.abort(live.turn, "incomplete").catch(function () {});
          setLive(live, "rolledBack", { code: "incomplete" });
        }
      }
      if (!cancelled) throw err;
      return null;
    });
  }

  /** Replays a known answer into the turn at a typing pace (the editor batches). */
  function replayInto(live, text, options) {
    var o = options || {};
    var guard = guardFor(live, o.action || "insert", o.selected);
    var all = String(text || "");
    var at = 0;
    return new Promise(function (resolve) {
      function tick() {
        if (live.code || live.stopping) return resolve();
        if (at >= all.length) return resolve(endTurn(live));
        var piece = all.slice(at, at + REPLAY_CHARS);
        at += piece.length;
        if (!send(live, guard, piece, o.cell)) return resolve();
        setTimeout(tick, REPLAY_MS);
      }
      tick();
    });
  }

  editorEvents.push(function () {
    editor.on("edit/state", function (params) {
      var held = params && turns[params.turn];
      if (!held) return;
      var live = held.live;
      if (params.state === "stopped" && live.state === "writing") {
        live.stopping = true;
        if (running) running.abort();
        setLive(live, "stopped");
      } else if (params.state === "rolledBack") {
        live.code = live.code || params.code || "engine";
        if (running) running.abort();
        setLive(live, "rolledBack", { code: live.code });
      } else if (params.state === "accepted" || params.state === "rejected") {
        live.stopping = true;
        setLive(live, params.state);
      }
    });
    editor.on("access/changed", reflectAccess);
  });

  // ---- Reading -------------------------------------------------------------

  /**
   * How much of the document to send, in estimated tokens: the provider's
   * context window (`initialize`'s `provider.contextTokens`; a local model
   * `LOCAL_CONTEXT` unless told more) less `reserve` (the answer's ceiling and
   * the prompt around the document), bounded by what is read at all. A FIXED
   * budget (it was 6,000) sent a large cloud model a fifth of a document it could
   * hold whole, and a small local one more than it could hold.
   */
  function contextBudget(reserve) {
    var p = (info && info.provider) || {};
    var window = Number(p.contextTokens) || 32000;
    if (p.local) window = Math.min(window, LOCAL_CONTEXT);
    return Math.max(800, Math.min(READ_CAP, window - reserve));
  }

  /** The note under an answer built from part of the document, or "". */
  function partNote(context) {
    if (!context.truncated) return "";
    return info && info.provider && info.provider.local ? s("truncatedLocal", { tokens: LOCAL_CONTEXT }) : s("truncated");
  }

  /** The containers besides the body an assistant reads, in the order their
   *  paragraphs are appended after the body's. Comments are not here: a comment
   *  is a conversation about the document, read by its own tool. */
  var SURFACE_KINDS = ["header", "footer", "footnote", "endnote", "text_box"];

  function call(tool, args) {
    return editor.tools.call(tool, args).then(function (answer) {
      if (!answer || !answer.ok) throw Object.assign(new Error("read"), { code: "read" });
      return answer.value;
    });
  }

  /**
   * The WHOLE document as the assistant's context reads it, up to READ_CAP
   * tokens (`docs/183` §23.6): the body's blocks; every table's cells
   * (`read_table`, attached to its block as `grid`, rows of `{text, paragraph}`,
   * since `read_blocks` summarises a table to its header); then the paragraphs
   * of every header, footer, footnote, endnote and text box (`read_blocks` on
   * that surface), each marked with its `surface` kind. A header repeated by
   * several sections is read once. `partial` is true when the cap stopped the
   * read, so the answer says it saw only part. O(document) reads, paged.
   */
  function readAll() {
    var blocks = [];
    var used = 0;
    var revision = 0;
    var partial = false;
    var full = function () { return used >= READ_CAP; };

    /** Pages `read_blocks` (on `surface`, or the body) into `blocks`. */
    function pages(surface, kind, seen) {
      function page(cursor) {
        var args = { maxTokens: 8000, format: "json" };
        if (surface) args.surface = surface;
        if (cursor) args.cursor = cursor;
        return call("read_blocks", args).then(function (value) {
          if (!surface) revision = value.revision;
          var list = value.blocks || [];
          for (var i = 0; i < list.length; i += 1) {
            var b = list[i];
            if (kind) {
              // The same header or footer text on several sections, once.
              var key = kind + "\u0000" + (b.text || "");
              if (b.kind === "paragraph" && seen[key]) continue;
              seen[key] = true;
              b = Object.assign({}, b, { surface: kind });
            }
            blocks.push(b);
            used += Core.estimate(b.text || "");
          }
          if (value.nextCursor && !full()) return page(value.nextCursor);
          if (value.nextCursor) partial = true;
          return list;
        });
      }
      return page(null);
    }

    /** Every table block's rows, paged by `read_table`, until the cap. */
    function tables(from) {
      var chain = Promise.resolve();
      blocks.slice(from).forEach(function (b) {
        if (b.kind !== "table" || b.grid) return;
        chain = chain.then(function () {
          if (full()) {
            partial = true;
            return null;
          }
          var rows = [];
          function page(cursor) {
            var args = { table: b.id, limit: 50 };
            if (cursor) args.cursor = cursor;
            return call("read_table", args).then(function (value) {
              (value.rows || []).forEach(function (row) {
                var cells = (row.cells || []).map(function (c) { return { text: String(c.text || ""), paragraph: c.paragraph || null }; });
                rows.push(cells);
                cells.forEach(function (c) { used += Core.estimate(c.text) + 1; });
              });
              if (value.nextCursor && !full()) return page(value.nextCursor);
              if (value.nextCursor) partial = true;
              return null;
            });
          }
          return page(null).then(function () { b.grid = rows; }, function () { b.grid = rows; });
        });
      });
      return chain;
    }

    return pages(null, "", null)
      .then(function () { return tables(0); })
      .then(function () {
        if (full()) {
          partial = true;
          return null;
        }
        return call("get_outline", { limit: 100 }).then(function (outline) {
          var seen = Object.create(null);
          var chain = Promise.resolve();
          (outline.surfaces || []).forEach(function (entry) {
            if (SURFACE_KINDS.indexOf(entry.kind) < 0 || !entry.paragraphs && !entry.tables) return;
            chain = chain.then(function () {
              if (full()) {
                partial = true;
                return null;
              }
              var from = blocks.length;
              // A surface this engine cannot read is skipped, not fatal: the
              // body alone still answers.
              return pages(entry.surface, entry.kind, seen).then(function () { return tables(from); }, function () { return null; });
            });
          });
          if (outline.surfacesNextCursor) partial = true;
          return chain;
        }, function () { return null; });
      })
      .then(function () { return { blocks: blocks, revision: revision, partial: partial }; });
  }

  function readFrom(node, maxTokens) {
    return editor.tools.call("read_blocks", { start: node, maxTokens: maxTokens || 800, format: "json" }).then(function (answer) {
      if (!answer || !answer.ok) throw Object.assign(new Error("read"), { code: "read" });
      return answer.value;
    });
  }

  /** Sends a plan to the private preview (replies only, now). */
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

  /** What a card says and offers when the access admits no change: the reason,
   *  in words, and the allowed alternative (a comment for a commenter, Copy for
   *  a reader). Resolves with the note. */
  function refusedEdit(target, text, sel) {
    var note = s(canComment() ? "access.cannotEdit" : "access.cannotComment", { mode: access.badgeText || "" });
    if (access.reasonText) note += " " + access.reasonText;
    var list = canComment() ? [{ id: "comment", label: s("comment"), run: function () { return addComment(target, sel, text); } }] : [];
    setActions(target, list, Core.cleanText(Core.stripCitations(text)), null);
    return Promise.resolve(note);
  }

  /** Insert at the cursor, live: one paragraph into the text at the caret, more
   *  as paragraphs, a list one per item (the same shaping `paragraphsOf` does). */
  function insertAtCaret(target, text) {
    if (!canEdit()) return refusedEdit(target, text, null);
    return caret().then(function (at) {
      if (!at) return s("noCaret");
      var paragraphs = Core.paragraphsOf(text);
      if (!paragraphs.length) return s("unchanged");
      return beginTurn(target, { kind: "insert", at: at }).then(function (live) {
        if (!live) return null;
        return replayInto(live, paragraphs.join("\n"), { action: "insert" }).then(function () { return null; });
      });
    });
  }

  /** The remembered one-paragraph selection as a replace spec, re-checked by the
   *  editor against the paragraph as it is NOW (the person may have typed). */
  function replaceSpec(sel) {
    var start = Math.min(sel.anchor.offset, sel.focus.offset);
    var end = Math.max(sel.anchor.offset, sel.focus.offset);
    return { kind: "replace", node: sel.anchor.node, start: start, end: end, exact: sel.text };
  }

  function replaceSelection(target, sel, text) {
    if (!canEdit()) return refusedEdit(target, text, sel);
    var clean = Core.cleanText(Core.stripCitations(text)).replace(/\s*\n+\s*/g, " ");
    if (!clean) return Promise.resolve(s("unchanged"));
    if (clean === sel.text) return Promise.resolve(s("unchanged"));
    return beginTurn(target, replaceSpec(sel)).then(function (live) {
      if (!live) return null;
      return replayInto(live, clean, { action: "replace", selected: sel.text }).then(function () { return null; });
    });
  }

  /** Add as comment, live: a comment by Casual AI on the attached selection, or
   *  on the caret's whole paragraph. */
  function addComment(target, sel, text) {
    if (!canComment()) return refusedEdit(target, text, sel);
    var body = Core.cleanText(Core.stripCitations(text)).slice(0, 4000);
    if (!body) return Promise.resolve(s("unchanged"));
    var spec = sel && singleParagraph(sel)
      ? Promise.resolve({ node: sel.anchor.node, start: Math.min(sel.anchor.offset, sel.focus.offset), end: Math.max(sel.anchor.offset, sel.focus.offset), exact: sel.text })
      : caret().then(function (at) {
        if (!at) return null;
        return readFrom(at.node, 200).then(function (value) {
          var block = (value.blocks || [])[0];
          return block ? { node: at.node, start: 0, end: bytes(block.text) } : null;
        });
      });
    return spec.then(function (where) {
      if (!where) return s("noCaret");
      return beginTurn(target, { kind: "at", at: { node: where.node, offset: where.end } }, "comment").then(function (live) {
        if (!live) return null;
        drainOut(live);
          return editor.edit.comment(live.turn, { node: where.node, start: where.start, end: where.end, exact: where.exact, text: body }).then(function (answer) {
          if (answer && answer.ok === false) {
            live.code = answer.code || "engine";
            void editor.edit.abort(live.turn, live.code);
            setLive(live, "rolledBack", { code: live.code });
            return null;
          }
          return endTurn(live).then(function () { return null; });
        });
      });
    });
  }

  var singleParagraph = function (sel) { return !!(sel && sel.anchor && sel.focus && sel.anchor.node === sel.focus.node); };

  /** The actions for an answer to a question, a summary or a draft. */
  function answerActions(target, answer, sel, again) {
    var plain = Core.cleanText(Core.stripCitations(answer));
    if (!canComment()) {
      setActions(target, [], plain, again);
      return;
    }
    var insert = { id: "insert", label: s("insert"), run: function () { return insertAtCaret(target, answer); } };
    var comment = { id: "comment", label: s("comment"), run: function () { return addComment(target, sel, answer); } };
    var list = canEdit() ? [insert, comment] : [comment];
    if (sel && singleParagraph(sel) && canEdit()) {
      var replace = { id: "replace", label: s("replace"), run: function () { return replaceSelection(target, sel, answer); } };
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
      void editor.ui.notify(s("workingOn", { action: title })).catch(function () {});
      userMessage(title + ": “" + clip(sel.text, 160) + "”");
      var out = card();
      if (!canEdit()) {
        // No change is admitted here: the model still answers, in the card, and
        // the card offers what is allowed instead.
        return complete(Core.selectionRequest(action, sel.text, language), out).then(function (answer) {
          var text = Core.cleanText(answer);
          out.body.hidden = true;
          out.extra.replaceChildren(diffView(sel.text, text));
          out.extra.hidden = false;
          return refusedEdit(out, text, sel).then(function (note) { out.note.textContent = note; });
        }).catch(function (err) { fail(err, out); });
      }
      var again = function () { return selectionAction(action, language); };
      if (singleParagraph(sel)) return rewriteOne(out, replaceSpec(sel), action, language, again);
      // Across paragraphs: the selected part of EACH paragraph is rewritten on
      // its own, streamed into one live turn (`selectionParts`). One request per
      // part, plain text in and out: the old single JSON answer for all of them
      // was the step a small model got wrong ("could not be used"), and it
      // rewrote the first and last paragraphs whole, outside the selection.
      return selectedParts(sel).then(function (found) {
        var skipped = found ? found.skipped : 0;
        if (!found || !found.parts.length) {
          out.note.textContent = skipped ? s("skipped", { count: skipped }) : s("noSelection");
          return null;
        }
        var run = found.parts.length === 1
          ? rewriteOne(out, partSpec(found.parts[0]), action, language, again)
          : rewriteParts(out, found.parts, action, language, again);
        return run.then(function () {
          if (skipped) out.note.textContent = s("skipped", { count: skipped });
        });
      }).catch(function (err) { fail(err, out); notifyError(err); });
    });
  }

  /** A part of a selection (`Core.selectionParts`) as a replace spec. */
  function partSpec(part) {
    return { kind: "replace", node: part.node, start: part.start, end: part.end, exact: part.exact };
  }

  /** One range rewritten LIVE: the pointer goes to it, marks it, and the
   *  rewrite streams in where it was (struck through first, when suggesting).
   *  Nothing changes until the first word arrives (`ai_coeditor.mjs`). */
  function rewriteOne(out, spec, action, language, again) {
    return beginTurn(out, spec).then(function (live) {
      if (!live) return null;
      return streamInto(live, Core.selectionRequest(action, spec.exact, language), { paragraphs: false, action: action, selected: spec.exact }).then(function (answer) {
        // A turn that was rolled back (nothing written, a violation, stale
        // text) has no diff to show: the live line already says why.
        if (answer === null || live.code) return;
        out.body.hidden = true;
        out.extra.replaceChildren(diffView(spec.exact, Core.cleanText(answer)));
        out.extra.hidden = false;
        setActions(out, [], Core.cleanText(answer), again);
      });
    }).catch(function (err) { fail(err, out); notifyError(err); });
  }

  /** The parts of a selection across paragraphs, read from its first end;
   *  a backward selection is read from its focus. Null when the selection is
   *  longer than one read can find. */
  function selectedParts(sel) {
    return readFrom(sel.anchor.node, 16000).then(function (value) {
      var found = Core.selectionParts(value.blocks || [], sel.anchor, sel.focus);
      if (found.complete) return found;
      return readFrom(sel.focus.node, 16000).then(function (back) {
        var reversed = Core.selectionParts(back.blocks || [], sel.focus, sel.anchor);
        return reversed.complete ? reversed : null;
      });
    });
  }

  /** Each part rewritten on its own, in ONE live turn (one undo step, one
   *  Accept all): the editor marks the part, the model's rewrite of exactly that
   *  text streams in, then the next. A part whose answer is empty stays as it
   *  was; a model failure rolls the whole turn back. */
  function rewriteParts(out, parts, action, language, again) {
    var first = parts[0];
    return beginTurn(out, { kind: "at", at: { node: first.node, offset: first.start } }).then(function (live) {
      if (!live) return null;
      var chain = Promise.resolve(true);
      parts.forEach(function (part) {
        chain = chain.then(function (go) {
          if (!go || live.code || live.stopping) return false;
          drainOut(live);
          return editor.edit.replace(live.turn, partSpec(part)).then(function (answer) {
            // The person changed this part meanwhile: it is left as it is.
            if (answer && answer.ok === false) return answer.code === "stale";
            return streamPart(live, Core.selectionRequest(action, part.exact, language), { paragraphs: false, action: action, selected: part.exact }).then(function (full) {
              return full !== null;
            });
          });
        });
      });
      return chain.then(function (go) {
        if (!go || live.code || live.stopping) return null;
        return endTurn(live).then(function () {
          if (!live.code) setActions(out, [], null, again);
        });
      });
    });
  }

  function paragraphsIn(blocks, lastNode) {
    var paras = [];
    for (var i = 0; i < blocks.length; i += 1) {
      if (blocks[i].kind === "paragraph") paras.push(blocks[i]);
      if (blocks[i].id === lastNode) break;
    }
    return paras;
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

  // ---- Tables (`docs/183` §23.4) ---------------------------------------------

  /** Builds `table` (`parseTableAnswer`'s) after `after`, then streams its
   *  cells in one by one, the pointer moving cell to cell. `remove` (the
   *  selected paragraphs) is removed after the table is built. */
  function buildTable(out, table, after, remove) {
    var spec = { kind: "table", after: after, rows: table.cells.length, columns: table.header.length, header: true };
    return beginTurn(out, spec).then(function (live) {
      if (!live) return null;
      var guard = guardFor(live, "table");
      return Promise.resolve().then(function () {
        var chain = Promise.resolve();
        table.cells.forEach(function (row, r) {
          row.forEach(function (text, c) {
            chain = chain.then(function () {
              if (live.code || live.stopping || !text) return null;
              return new Promise(function (resolve) {
                var at = 0;
                var first = true;
                (function tick() {
                  if (live.code || live.stopping || at >= text.length) return resolve();
                  var piece = text.slice(at, at + REPLAY_CHARS);
                  at += piece.length;
                  if (!send(live, guard, piece, first ? [r, c] : null)) return resolve();
                  first = false;
                  setTimeout(tick, REPLAY_MS);
                })();
              });
            });
          });
        });
        // The paragraphs the table was made from go last, in the same turn, so
        // one Undo (or Reject all) puts them back with the table gone.
        if (remove) {
          chain = chain.then(function () {
            if (live.code || live.stopping) return null;
            drainOut(live);
          return editor.edit.remove(live.turn, remove);
          });
        }
        return chain.then(function () { return endTurn(live); });
      });
    });
  }

  /** "Turn into table": the selected paragraphs, or a prompt at the cursor. */
  function turnIntoTable(prompt, retry) {
    return editor.selection().then(function (sel) {
      var title = s("command.table");
      if (!retry) userMessage(prompt ? prompt : title + (sel && sel.hasRange ? ": “" + clip(sel.text, 120) + "”" : ""));
      var out = card();
      if (!canEdit()) {
        out.note.textContent = s(canComment() ? "access.cannotEdit" : "access.cannotComment", { mode: access.badgeText || "" }) + (access.reasonText ? " " + access.reasonText : "");
        return null;
      }
      var at = sel && (sel.focus || sel.anchor);
      if (!at || !at.node) {
        out.note.textContent = s("noCaret");
        return null;
      }
      var withSelection = !!(sel.hasRange && sel.text.trim());
      var start = withSelection ? sel.anchor.node : at.node;
      return readFrom(start, 8000).then(function (value) {
        var blocks = value.blocks || [];
        var paras = withSelection ? paragraphsIn(blocks, sel.focus.node) : [];
        var context = withSelection ? null : Core.selectContext(blocks, prompt || "", 1000);
        out.body.textContent = s("working");
        return complete(Core.tableRequest(paras, prompt, context), null).then(function (answer) {
          out.body.textContent = "";
          var table = Core.parseTableAnswer(answer);
          if (!table.ok) {
            // Say WHICH: the model did not answer in the structured form asked for.
            out.note.textContent = s("unparsableFormat");
            setActions(out, [], null, function () { return turnIntoTable(prompt, true); });
            return null;
          }
          var last = paras[paras.length - 1];
          var after = paras.length ? last.id : at.node;
          var remove = paras.length ? { anchor: { node: paras[0].id, offset: 0 }, focus: { node: last.id, offset: bytes(last.text) } } : null;
          return buildTable(out, table, after, remove).then(function () {
            setActions(out, [], table.cells.map(function (row) { return row.join("\t"); }).join("\n"), function () { return turnIntoTable(prompt, true); });
          });
        });
      }).catch(function (err) { fail(err, out); });
    });
  }

  /** "Describe this table": the table as structured data; a paragraph streams
   *  in below it (or, confirmed, replaces it). */
  function describeTable(kind, retry) {
    return editor.selection().then(function (sel) {
      var table = sel && sel.table;
      if (!retry) userMessage(s("command.describeTable"));
      var out = card();
      if (!table) {
        out.note.textContent = s("noTable");
        return null;
      }
      return editor.tools.call("read_table", { table: table, limit: Core.TABLE_LIMITS.rows }).then(function (answer) {
        if (!answer || !answer.ok) throw Object.assign(new Error("read"), { code: "read" });
        var rows = Core.tableRows(answer.value);
        var request = Core.describeTableRequest(rows, kind);
        if (!canEdit()) {
          return complete(request, out).then(function (text) {
            out.body.textContent = Core.cleanText(text);
            return refusedEdit(out, text, null).then(function (note) { out.note.textContent = note; });
          });
        }
        return beginTurn(out, { kind: "after", block: table }).then(function (live) {
          if (!live) return null;
          return streamInto(live, request, { paragraphs: true, action: "describe" }).then(function (text) {
            if (text === null) return;
            var clean = Core.cleanText(text);
            // Replace the table instead, after a confirmation (a second press).
            var confirming = false;
            var replaceAction = {
              id: "replaceTable",
              label: s("replaceTable"),
              run: function () {
                if (!confirming) {
                  confirming = true;
                  var b = out.actions.querySelector('[data-action="replaceTable"]');
                  if (b) b.textContent = s("replaceTableConfirm");
                  return s("replaceTableConfirm");
                }
                var next = card();
                return beginTurn(next, { kind: "replaceTable", table: table }).then(function (again) {
                  if (!again) return null;
                  return replayInto(again, Core.paragraphsOf(clean).join("\n"), { action: "describe" }).then(function () { return null; });
                });
              },
            };
            setActions(out, live.mode === "direct" || live.mode === "suggest" ? [replaceAction] : [], clean, function () { return describeTable(kind, true); });
          });
        });
      }).catch(function (err) { fail(err, out); });
    });
  }

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
      var prior = Core.trimHistory(history, 12);
      var spoken = prior.reduce(function (n, h) { return n + Core.estimate(h.content); }, Core.estimate(question + (sel ? sel.text : "")));
      var context = Core.selectContext(doc.blocks, question + (sel ? " " + sel.text : ""), contextBudget(1500 + 800 + spoken), doc.partial);
      return complete(Core.askRequest(question, context, prior, sel ? sel.text : null), out).then(function (answer) {
        renderAnswer(out, answer, context.ids);
        if (context.truncated) out.note.textContent = partNote(context);
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
      var context = Core.selectContext(doc.blocks, "", contextBudget(1200 + 800), doc.partial);
      return complete(Core.summaryRequest(kind, context), out).then(function (answer) {
        renderAnswer(out, answer, context.ids);
        if (context.truncated) out.note.textContent = partNote(context);
        answerActions(out, answer, null, function () { return summarise(kind, true); });
        // Part of the discussion a later "write a paragraph from this" draws on.
        if (!retry) history.push({ role: "user", content: s("chip." + kind) }, { role: "assistant", content: Core.stripCitations(answer) });
        persist();
      });
    }).catch(function (err) { setWorking(""); fail(err, out); });
  }

  /** A review's findings applied in ONE live turn: for each, the pointer goes to
   *  the quote, replaces it (when the finding suggests text and the access
   *  admits it), and leaves the comment. */
  function applyReview(out, items, byId) {
    if (!items.length) return Promise.resolve(s("unchanged"));
    var first = items[0];
    var kind = canEdit() ? "text" : "comment";
    return beginTurn(out, { kind: "at", at: { node: first.node, offset: 0 } }, kind).then(function (live) {
      if (!live) return null;
      var guard = guardFor(live, "review");
      var skipped = 0;
      var chain = Promise.resolve();
      items.forEach(function (item) {
        chain = chain.then(function () {
          if (live.code || live.stopping) return null;
          var block = byId[item.node];
          var at = block ? block.text.indexOf(item.quote) : -1;
          if (at < 0) {
            skipped += 1;
            return null;
          }
          var start = bytes(block.text.slice(0, at));
          var end = start + bytes(item.quote);
          var commentOn = function (to) {
            drainOut(live);
          return editor.edit.comment(live.turn, { node: item.node, start: start, end: to, text: item.comment }).then(function (answer) {
              if (answer && answer.ok === false) skipped += 1;
            });
          };
          if (!item.replacement || !canEdit()) return commentOn(end);
          drainOut(live);
          return editor.edit.replace(live.turn, { node: item.node, start: start, end: end, exact: item.quote }).then(function (answer) {
            if (answer && answer.ok === false) {
              skipped += 1;
              return null;
            }
            if (!send(live, guard, item.replacement)) return null;
            // The new text starts where the quote did; a struck quote has no
            // width in the editor's positions, so the comment covers the new text.
            var to = start + bytes(item.replacement);
            return commentOn(to);
          });
        });
      });
      return chain.then(function () { return endTurn(live); }).then(function () {
        return skipped ? s("dropped", { count: skipped }) : null;
      });
    });
  }

  function review(retry) {
    if (!retry) userMessage(s("chip.review"));
    var out = card();
    setWorking(s("reading"));
    return readAll().then(function (doc) {
      var context = Core.selectContext(doc.blocks, "", contextBudget(4000 + 900), doc.partial);
      var byId = {};
      doc.blocks.forEach(function (b) { if (b.kind === "paragraph") byId[b.id] = b; });
      // A finding may quote a table cell: its paragraph is checked against the
      // cell's text, and the engine re-checks the quote before anything lands.
      Object.keys(context.cells || {}).forEach(function (id) { if (!byId[id]) byId[id] = { id: id, text: context.cells[id] }; });
      return complete(Core.reviewRequest(context), out).then(function (answer) {
        out.body.textContent = "";
        var result = Core.planForReview(answer, context, byId, doc.revision);
        if (!result.ok) {
          out.note.textContent = s(result.reason === "nothing" ? "unchanged" : "unparsableFormat");
          setActions(out, [], null, function () { return review(true); });
          return;
        }
        // Each finding, shown before anything is applied: the suggested change
        // as a diff, or the quoted text, and the comment under it.
        var list = document.createElement("ul");
        list.className = "ca-review";
        var copy = [];
        var findings = [];
        result.plan.edits.forEach(function (edit) {
          if (edit.op === "comment") findings.push({ node: edit.target.node, quote: edit.target.exact, comment: edit.text, replacement: null });
          else if (edit.op === "replace_text" && findings.length) findings[findings.length - 1].replacement = edit.text;
        });
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
        var actions = [];
        if (canComment()) {
          var label = canEdit() ? s(access.mode === "direct" ? "applyDirect" : "apply") : s("applyComments");
          actions.push({ id: "apply", label: label, run: function () { return applyReview(out, findings, byId); } });
        } else out.note.textContent = summary + " " + s("access.cannotComment", { mode: access.badgeText || "" });
        setActions(out, actions, copy.join("\n\n"), function () { return review(true); });
      });
    }).catch(function (err) { setWorking(""); fail(err, out); });
  }

  /** Draft at the cursor, LIVE: the text streams into the document. */
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
        var request = Core.draftRequest(prompt, context, Core.trimHistory(history, 12));
        if (!canEdit()) {
          // The draft is still written, in the card, for the reader to copy.
          return complete(request, out).then(function (answer) {
            out.body.textContent = Core.cleanText(answer);
            return refusedEdit(out, answer, null).then(function (note) { out.note.textContent = note; });
          });
        }
        return beginTurn(out, { kind: "insert", at: at }).then(function (live) {
          if (!live) return null;
          return streamInto(live, request, { paragraphs: true, action: "draft" }).then(function (answer) {
            if (answer === null) return;
            setActions(out, [], Core.cleanText(answer), function () { return draft(prompt, true); });
            if (!retry) history.push({ role: "user", content: prompt }, { role: "assistant", content: Core.cleanText(answer) });
            persist();
          });
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

  /** A reply on a thread still goes through the private preview: a reply joins
   *  someone else's conversation, so the person sees it before it is posted. */
  function draftReply(comment, all, retry) {
    var thread = [comment].concat(all.filter(function (c) { return c.parentParaId && c.parentParaId === comment.paraId; }));
    if (!retry) userMessage(s("replyFor", { text: clip(comment.text, 80) }));
    var out = card();
    return readFrom(comment.anchor.node, 400).then(function (value) {
      var block = (value.blocks || [])[0];
      var quoted = block ? block.text.slice(comment.anchor.start, comment.anchor.end) : "";
      return complete(Core.replyRequest(thread, quoted), out).then(function (answer) {
        var reply = { id: "comment", label: s("comment"), run: function () { return propose(Core.planForReply(comment, answer, value.revision)); } };
        setActions(out, canComment() ? [reply] : [], Core.cleanText(answer), function () { return draftReply(comment, all, true); });
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
    $("tableMode").hidden = mode !== "table";
    $("prompt").placeholder = s(mode === "draft" ? "prompt.draftPlaceholder" : mode === "table" ? "prompt.tablePlaceholder" : "prompt.placeholder");
  }

  /** The chip names the selection only while there is one; the table chips
   *  offer themselves where the selection fits (paragraphs, or a table). */
  function reflectSelection() {
    return editor.selection().then(function (sel) {
      attached = sel && sel.hasRange && sel.text && sel.text.trim() ? { text: sel.text, anchor: sel.anchor, focus: sel.focus, table: sel.table || null } : null;
      caretTable = sel && sel.table ? sel.table : null;
      $("attached").hidden = !attached;
      if (attached) {
        var label = s("attached", { text: clip(attached.text.replace(/\s+/g, " ").trim(), 60) });
        $("attachedText").textContent = label;
        $("attached").title = label;
      }
      $("turnIntoTable").hidden = !(attached && !attached.table && canEdit());
      $("describeTableChip").hidden = !caretTable;
    }).catch(function () {});
  }

  function grow() {
    var field = $("prompt");
    field.style.height = "auto";
    field.style.height = Math.min(field.scrollHeight, 160) + "px";
  }

  function send_() {
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
    } else if (mode === "table") {
      setMode("ask");
      void turnIntoTable(text);
    } else void ask(text, attached);
  }
  $("send").addEventListener("click", send_);
  $("prompt").addEventListener("input", grow);
  $("prompt").addEventListener("keydown", function (event) {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      send_();
    } else if (event.key === "Escape" && running) {
      event.preventDefault();
      running.abort();
    }
  });
  $("detach").addEventListener("click", function () {
    attached = null;
    $("attached").hidden = true;
    $("turnIntoTable").hidden = true;
    $("prompt").focus();
  });
  $("draftCancel").addEventListener("click", function () {
    setMode("ask");
    $("prompt").focus();
  });
  $("tableCancel").addEventListener("click", function () {
    setMode("ask");
    $("prompt").focus();
  });
  $("turnIntoTable").addEventListener("click", function () { if (!running) void turnIntoTable(null); });
  $("describeTableChip").addEventListener("click", function () { if (!running) void describeTable("describe"); });
  Array.prototype.forEach.call(document.querySelectorAll("[data-chip]"), function (chip) {
    chip.addEventListener("click", function () {
      if (running) return;
      var kind = chip.dataset.chip;
      if (kind === "review") void review();
      else if (kind === "draft") {
        setMode("draft");
        $("prompt").focus();
      } else if (kind === "table") {
        setMode("table");
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
    $("tableModeText").textContent = s("tableMode");
    $("tableCancel").setAttribute("aria-label", s("draftCancel"));
    $("tableCancel").title = s("draftCancel");
    $("turnIntoTableText").textContent = s("chip.turnIntoTable");
    $("describeTableText").textContent = s("chip.describeTable");
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
        return refreshAccess();
      }).then(restore);
    }).then(function () {
      editorEvents.forEach(function (fn) { fn(); });
      void reflectSelection();
      editor.on("selection/changed", function () { void reflectSelection(); });
      editor.on("contribution/invoked", function (params) {
        var command = params && params.command;
        // Ask and Translate need the panel; the host opens it for everything that
        // is not a selection action, so these two open it themselves.
        if (command === "ask" || command === "translate") void editor.ui.panel(true).catch(function () {});
        void refreshAccess().then(function () {
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
          else if (command === "table") void turnIntoTable(null);
          else if (command === "describeTable") void describeTable("describe");
          else if (Core.ACTION_IDS.indexOf(command) >= 0) void selectionAction(command);
        });
      });
      editor.on("document/opened", function (params) {
        info.documentKey = params.documentKey;
        void restore();
      });
      editor.on("theme/changed", function (params) {
        if (params && params.locale && params.locale !== info.locale) info.locale = params.locale;
      });
      window.casualAssistant = { ask: ask, summarise: summarise, review: review, selectionAction: selectionAction, draft: draft, replies: replies, clear: clearHistory, turnIntoTable: turnIntoTable, describeTable: describeTable, access: function () { return access; } };
    });
  });
})();
