// SPDX-License-Identifier: Apache-2.0
// The OpenDoc plugin client (`docs/183` §5): a classic script a plugin's page
// includes to talk to the editor. Copy it next to your plugin — a plugin's CSP
// lets it load scripts from its own origin only.
//
//   <script src="opendoc-plugin.js"></script>
//   <script>
//     OpenDocPlugin.connect().then(async (editor) => {
//       const info = await editor.initialize();
//       editor.on("contribution/invoked", async ({ command }) => {
//         const selection = await editor.selection();
//         await editor.tools.call("propose_edits", { base_revision: 0, edits: [ ... ] });
//       });
//     });
//   </script>
//
// JSON-RPC 2.0 over the one MessagePort the editor hands the frame at its first
// load. Every method resolves with the result or rejects with an Error whose
// `code` is the editor's refusal code (`permission-denied`, `rate-limited`, …).
(function () {
  "use strict";
  var connecting = null;

  function connect() {
    if (connecting) return connecting;
    connecting = new Promise(function (resolve) {
      function onMessage(event) {
        var data = event.data;
        if (!data || data.opendocPlugin !== 1 || data.type !== "connect" || !event.ports || !event.ports[0]) return;
        window.removeEventListener("message", onMessage);
        resolve(client(event.ports[0]));
      }
      window.addEventListener("message", onMessage);
    });
    return connecting;
  }

  function client(port) {
    var next = 1;
    var waiting = new Map();
    var listeners = new Map();
    port.onmessage = function (event) {
      var message = event.data;
      if (!message || message.jsonrpc !== "2.0") return;
      if (message.id !== undefined && message.id !== null && waiting.has(message.id)) {
        var waiter = waiting.get(message.id);
        waiting.delete(message.id);
        if (message.error) {
          var error = new Error(message.error.message);
          error.code = (message.error.data && message.error.data.code) || "error";
          error.data = message.error.data || {};
          waiter.reject(error);
        } else {
          waiter.resolve(message.result);
        }
        return;
      }
      if (message.method) {
        var set = listeners.get(message.method);
        if (set) set.forEach(function (fn) { try { fn(message.params || {}); } catch (err) { console.error(err); } });
      }
    };
    function request(method, params) {
      var id = next++;
      return new Promise(function (resolve, reject) {
        waiting.set(id, { resolve: resolve, reject: reject });
        port.postMessage({ jsonrpc: "2.0", id: id, method: method, params: params || {} });
      });
    }
    function on(method, fn) {
      if (!listeners.has(method)) listeners.set(method, new Set());
      listeners.get(method).add(fn);
      return function () { listeners.get(method).delete(fn); };
    }
    return {
      request: request,
      on: on,
      initialize: function () { return request("initialize"); },
      selection: function () { return request("selection/get"); },
      tools: {
        list: function () { return request("tools/list").then(function (r) { return r.tools; }); },
        call: function (name, args) { return request("tools/call", { name: name, arguments: args || {} }); },
      },
      commands: {
        list: function () { return request("commands/list").then(function (r) { return r.commands; }); },
        run: function (id) { return request("commands/run", { id: id }); },
      },
      ai: {
        // `request` is an OpenAI Chat Completions body. With `stream: true`,
        // `options.onChunk` receives each chat.completion.chunk.
        complete: function (body, options) {
          var id = next;
          var off = null;
          if (body && body.stream && options && options.onChunk) {
            off = on("ai/chunk", function (params) { if (params.requestId === id) options.onChunk(params.chunk); });
          }
          if (options && options.signal) {
            options.signal.addEventListener("abort", function () { request("ai/cancel", { requestId: id }).catch(function () {}); }, { once: true });
          }
          return request("ai/complete", { request: body }).finally(function () { if (off) off(); });
        },
      },
      ui: {
        notify: function (message, kind) { return request("ui/notify", { message: message, kind: kind || "info" }); },
        panel: function (open) { return request("ui/panel", { open: open !== false }); },
      },
      storage: {
        get: function () { return request("storage/get").then(function (r) { return r.value; }); },
        set: function (value) { return request("storage/set", { value: value }); },
      },
    };
  }

  window.OpenDocPlugin = { connect: connect };
})();
