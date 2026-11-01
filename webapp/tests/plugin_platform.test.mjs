// The plugin platform's guarantees (`docs/183` §4–§7), in node: the manifest is
// checked field by field, the sandbox and its CSP are what the design says, the
// grant is an intersection, and a running plugin reaches the document only
// through the same tools MCP and WebMCP use — through the private preview, never
// apply — over a real MessagePort.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import {
  PLUGIN_PERMISSIONS,
  SANDBOX_TOKENS,
  addedPermissions,
  checkManifest,
  effectiveGrant,
  localised,
  sandboxCsp,
  sandboxDocument,
} from "../src/plugin_manifest.mjs";
import { RPC_ERRORS, servePort } from "../src/plugin_rpc.mjs";
import { CONNECT_MESSAGE, commandPermission, grantAdmits, startPlugin } from "../src/plugin_host.mjs";
import { admitInOrder, catalogueMode, checkCatalogue, loadCatalogues } from "../src/plugin_catalogue.mjs";
import { COMMAND_FAMILIES, commandContract } from "../src/host_contract.mjs";

const where = { manifestUrl: "https://plugins.example/lt/manifest.json", selfOrigin: "https://docs.example" };
const base = () => ({
  manifestVersion: 1,
  id: "org.example.tool",
  name: "Tool",
  version: "1.0.0",
  entry: "index.html",
  permissions: ["read", "suggest"],
  contributes: { commands: [{ id: "run", title: "Run it" }], ribbon: [{ command: "run", icon: "bolt" }], panel: { title: "Tool" } },
});

// ---- The manifest ------------------------------------------------------------

test("a good manifest resolves its entry against the manifest URL", () => {
  const out = checkManifest(base(), where);
  assert.equal(out.ok, true, out.reason);
  assert.equal(out.manifest.entryUrl, "https://plugins.example/lt/index.html");
  assert.equal(out.manifest.entryOrigin, "https://plugins.example");
  assert.equal(out.manifest.contributes.panel.width, 320);
});

test("an unknown field is refused, not ignored", () => {
  assert.equal(checkManifest({ ...base(), runInPage: true }, where).ok, false);
  assert.equal(checkManifest({ ...base(), contributes: { ...base().contributes, toolbar: [] } }, where).ok, false);
});

test("permissions come from the six, once each", () => {
  assert.deepEqual(PLUGIN_PERMISSIONS, ["read", "comment", "suggest", "edit", "network", "ai"]);
  assert.equal(checkManifest({ ...base(), permissions: ["read", "admin"] }, where).ok, false);
  assert.equal(checkManifest({ ...base(), permissions: ["read", "read"] }, where).ok, false);
});

test("network needs origins, exactly, and origins need network", () => {
  assert.equal(checkManifest({ ...base(), permissions: ["read", "network"] }, where).ok, false);
  assert.equal(checkManifest({ ...base(), network: ["https://api.example.org"] }, where).ok, false);
  assert.equal(checkManifest({ ...base(), permissions: ["network"], network: ["https://*.example.org"] }, where).ok, false);
  assert.equal(checkManifest({ ...base(), permissions: ["network"], network: ["https://api.example.org/v2"] }, where).ok, false);
  assert.equal(checkManifest({ ...base(), permissions: ["network"], network: ["http://evil.example"] }, where).ok, false);
  const ok = checkManifest({ ...base(), permissions: ["read", "network"], network: ["https://api.example.org"] }, where);
  assert.deepEqual(ok.manifest.network, ["https://api.example.org"]);
});

test("an entry must be https (or this computer), never javascript: or data:", () => {
  for (const entry of ["javascript:alert(1)", "data:text/html,hi", "http://evil.example/x.html", "https://user:pw@x.example/"]) {
    assert.equal(checkManifest({ ...base(), entry }, where).ok, false, entry);
  }
  assert.equal(checkManifest({ ...base(), entry: "http://localhost:8080/p.html" }, where).ok, true);
});

test("contributions must name declared commands", () => {
  assert.equal(checkManifest({ ...base(), contributes: { commands: [{ id: "run", title: "Run" }], ribbon: [{ command: "other" }] } }, where).ok, false);
  assert.equal(checkManifest({ ...base(), contributes: { commands: [{ id: "run", title: "Run" }, { id: "run", title: "Again" }] } }, where).ok, false);
  assert.equal(checkManifest({ ...base(), contributes: { commands: [{ id: "run", title: "Run" }], contextMenu: [{ command: "run", when: "hover" }] } }, where).ok, false);
});

test("manifest strings localise, falling back to the language then English", () => {
  const { manifest } = checkManifest({ ...base(), locales: { de: { name: "Werkzeug", "commands.run": "Ausführen" } } }, where);
  assert.equal(localised(manifest, "name", "de-AT", manifest.name), "Werkzeug");
  assert.equal(localised(manifest, "commands.run", "de", "Run it"), "Ausführen");
  assert.equal(localised(manifest, "name", "fr", manifest.name), "Tool");
  assert.equal(checkManifest({ ...base(), locales: { de: { "commands.nope": "x" } } }, where).ok, false);
});

test("the schema file and the checker agree on the top-level fields and permissions", () => {
  const schema = JSON.parse(readFileSync(new URL("../schemas/plugin-manifest.schema.json", import.meta.url), "utf8"));
  assert.deepEqual(schema.properties.permissions.items.enum, [...PLUGIN_PERMISSIONS]);
  assert.equal(schema.additionalProperties, false);
  for (const field of Object.keys(schema.properties)) {
    assert.notEqual(checkManifest({ ...base(), [field]: base()[field] }, where).reason?.includes("is not a manifest field"), true, field);
  }
});

// ---- The sandbox -----------------------------------------------------------------

test("the frame may run scripts and nothing else", () => {
  assert.deepEqual(SANDBOX_TOKENS, ["allow-scripts"]);
  assert.equal(SANDBOX_TOKENS.includes("allow-same-origin"), false, "same-origin would let a plugin read the key");
});

test("the CSP connects nowhere unless network is held, and then only to the declared origins", () => {
  const { manifest } = checkManifest({ ...base(), permissions: ["read", "network"], network: ["https://api.example.org"] }, where);
  assert.match(sandboxCsp(manifest, ["read"]), /connect-src 'none'/);
  assert.match(sandboxCsp(manifest, ["read", "network"]), /connect-src https:\/\/api\.example\.org;/);
  const csp = sandboxCsp(manifest, ["read"]);
  for (const directive of ["default-src 'none'", "frame-src 'none'", "form-action 'none'", "object-src 'none'", "worker-src 'none'"]) {
    assert.ok(csp.includes(directive), directive);
  }
  assert.match(csp, /script-src https:\/\/plugins\.example 'unsafe-inline'/);
});

test("the CSP and the base come before the plugin's own markup", () => {
  const { manifest } = checkManifest(base(), where);
  const html = sandboxDocument("<!doctype html><title>x</title><script>evil()</script>", manifest, ["read"]);
  const csp = html.indexOf("Content-Security-Policy");
  assert.ok(csp > 0 && csp < html.indexOf("<script>"));
  assert.ok(html.startsWith("<!doctype html>"));
  assert.match(html, /<base href="https:\/\/plugins\.example\/lt\/index\.html">/);
  // A manifest URL cannot break out of the attribute.
  const sneaky = sandboxDocument("", { ...manifest, entryUrl: 'https://x.example/"><script>1</script>' }, []);
  assert.equal(sneaky.includes('"><script>1'), false);
});

// ---- The grant ------------------------------------------------------------------

test("the effective grant is declared ∩ granted, capped by the mount", () => {
  const owner = { access: "review", direct: false };
  assert.deepEqual(effectiveGrant(["read", "suggest"], ["read", "suggest"], owner), { permissions: ["read", "suggest"], access: "suggest", direct: false });
  // Granted more than declared: the manifest is the ceiling.
  assert.equal(effectiveGrant(["read"], ["read", "edit"], owner).access, "read");
  // A readonly mount: reads only, whatever the plugin was granted.
  const readonly = effectiveGrant(["read", "suggest", "comment"], ["read", "suggest", "comment"], { access: "read", direct: false });
  assert.equal(readonly.access, "read");
  assert.deepEqual(readonly.permissions, ["read"]);
  // Never review: deciding tracked changes is a person's.
  assert.equal(effectiveGrant(["read", "edit"], ["read", "edit"], owner).access, "suggest");
  // Direct only with edit AND the host's direct grant.
  assert.equal(effectiveGrant(["read", "edit"], ["read", "edit"], { access: "suggest", direct: true }).direct, true);
  assert.equal(effectiveGrant(["read", "suggest"], ["read", "suggest"], { access: "suggest", direct: true }).direct, false);
  // No read, no editing.
  assert.equal(effectiveGrant(["suggest", "ai"], ["suggest", "ai"], owner).access, "none");
  assert.deepEqual(effectiveGrant(["suggest", "ai"], ["suggest", "ai"], owner).permissions, ["ai"]);
});

test("a new version asks only for what it adds", () => {
  assert.deepEqual(addedPermissions(["read"], ["read", "ai"]), ["ai"]);
  assert.deepEqual(addedPermissions(["read", "ai"], ["read"]), []);
});

test("commands by id: no file, session, plugin or AI commands; mutations need edit", () => {
  assert.equal(commandPermission("file.save", commandContract("file.save")), null);
  assert.equal(commandPermission("file.print", commandContract("file.print")), null);
  assert.equal(commandPermission("plugin.manage", commandContract("plugin.manage")), null);
  assert.equal(commandPermission("ai.anything", { requires: null }), null);
  assert.equal(commandPermission("format.bold", commandContract("format.bold")), "edit");
  assert.equal(commandPermission("view.outline", commandContract("view.outline")), "none");
  assert.equal(grantAdmits("edit", ["read", "suggest"]), false);
  assert.equal(grantAdmits("none", []), true);
  assert.equal(grantAdmits(null, PLUGIN_PERMISSIONS), false);
});

test("the plugin. family is declared and requires nothing of the host", () => {
  const row = COMMAND_FAMILIES.find((family) => family.prefix === "plugin.");
  assert.ok(row);
  assert.equal(row.requires, null);
  assert.equal(commandContract("plugin.org.example.tool.run").family, "plugin.");
});

// ---- Catalogues --------------------------------------------------------------------

test("?plugins= decides the source: none, a host catalogue, or standalone; a frame runs none by default", () => {
  assert.deepEqual(catalogueMode({ plugins: "none" }), { mode: "none" });
  assert.deepEqual(catalogueMode({ plugins: "https://h.example/c.json" }), { mode: "host", url: "https://h.example/c.json" });
  assert.deepEqual(catalogueMode({ framed: true }), { mode: "none" });
  assert.deepEqual(catalogueMode({}), { mode: "standalone" });
});

test("a host catalogue carries its grants; unknown fields are refused", () => {
  const ok = checkCatalogue({ catalogueVersion: 1, plugins: [{ manifest: "lt/manifest.json", grant: ["read"] }] }, "https://h.example/c.json", { source: "host" });
  assert.equal(ok.entries[0].manifestUrl, "https://h.example/lt/manifest.json");
  assert.deepEqual(ok.entries[0].hostGrant, ["read"]);
  assert.equal(checkCatalogue({ catalogueVersion: 1, plugins: [], autoRun: true }, "https://h.example/c.json", { source: "host" }).ok, false);
});

test("the first-party catalogue is valid and lists nothing that is not built", async () => {
  const raw = JSON.parse(readFileSync(new URL("../plugins/catalogue.json", import.meta.url), "utf8"));
  assert.equal(checkCatalogue(raw, "https://docs.example/plugins/catalogue.json", { source: "first-party" }).ok, true);
  const out = await loadCatalogues({ config: {}, baseUrl: "https://docs.example/editor.html", fetchJson: async () => raw, view: { localStorage: { getItem: () => null } } });
  assert.equal(out.allowInstall, true);
});

test("?plugins=none fetches nothing at all", async () => {
  let fetched = 0;
  const out = await loadCatalogues({ config: { plugins: "none" }, baseUrl: "https://d.example/", fetchJson: async () => (fetched += 1), view: {} });
  assert.equal(out.entries.length, 0);
  assert.equal(fetched, 0);
});

test("every manifest downloads at once, and admission still follows catalogue order (HF-421)", async () => {
  // A barrier: no manifest answers until all three have been ASKED for. A loader
  // that awaits one manifest before requesting the next never gets past the
  // first, so this fails on the serial loop rather than measuring a time.
  const entries = ["a", "b", "c"].map((id) => ({ manifestUrl: `https://p.example/${id}/manifest.json`, id }));
  const asked = [];
  let release;
  const everyoneAsked = new Promise((resolve) => (release = resolve));
  // Answers land in REVERSE order once released, so order-by-arrival would show.
  const fetchJson = async (url) => {
    asked.push(url);
    if (asked.length === entries.length) release();
    await everyoneAsked;
    await new Promise((resolve) => setTimeout(resolve, 10 * (entries.length - asked.indexOf(url))));
    if (url.includes("/b/")) throw new Error("404");
    return { url };
  };
  const admitted = [];
  const admit = async (entry, fetched) => {
    try {
      admitted.push([entry.id, (await fetched).url === entry.manifestUrl]);
    } catch {
      admitted.push([entry.id, "unavailable"]);
    }
    return entry.id;
  };
  const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error(`manifests fetched one at a time: only ${asked.length} of 3 asked for`)), 2000).unref());
  const out = await Promise.race([admitInOrder(entries, fetchJson, admit), timeout]);
  assert.deepEqual(out, ["a", "b", "c"]);
  assert.deepEqual(admitted, [["a", true], ["b", "unavailable"], ["c", true]]);
});

// ---- The RPC -------------------------------------------------------------------------

/** One end served by `handlers`, the other a tiny client. */
/** Every port a test opened, closed after it whether it passed or not: an open
 *  MessagePort keeps node alive, so a failing assertion must not hang the run. */
const opened = [];
test.afterEach(() => {
  for (const close of opened.splice(0)) close();
});

function pair(handlers, options) {
  const { port1, port2 } = new MessageChannel();
  const server = servePort(port1, handlers, options);
  let next = 1;
  const waiting = new Map();
  const notes = [];
  port2.onmessage = ({ data }) => {
    if (data.id !== undefined && waiting.has(data.id)) {
      waiting.get(data.id)(data);
      waiting.delete(data.id);
    } else notes.push(data);
  };
  const call = (method, params) =>
    new Promise((resolve) => {
      const id = next++;
      waiting.set(id, resolve);
      port2.postMessage({ jsonrpc: "2.0", id, method, params });
    });
  const close = () => { server.close(); port2.close(); };
  opened.push(close);
  return { server, call, notes, close };
}

test("rpc: results, unknown methods and refusals are values with codes", async () => {
  const { call, close } = pair({ echo: (p) => p, boom: () => { throw new Error("nope"); } });
  assert.deepEqual((await call("echo", { a: 1 })).result, { a: 1 });
  assert.equal((await call("missing")).error.code, RPC_ERRORS.methodNotFound);
  assert.equal((await call("boom")).error.data.code, "internal");
  assert.equal((await call("__proto__")).error.code, RPC_ERRORS.methodNotFound, "no prototype lookups");
  close();
});

test("rpc: more than the per-second budget is refused rate-limited", async () => {
  const { call, close } = pair({ echo: () => 1 }, { bounds: { maxBytes: 1e6, maxInFlight: 64, perSecond: 3 }, now: () => 1000 });
  const answers = await Promise.all([1, 2, 3, 4, 5].map(() => call("echo")));
  assert.equal(answers.filter((a) => a.error?.data?.code === "rate-limited").length, 2);
  close();
});

test("rpc: the live co-editor's stream has its own window, so it cannot starve the add-in's other calls", async () => {
  const bounds = { maxBytes: 1e6, maxInFlight: 64, perSecond: 3, streamPerSecond: 5 };
  const { call, close } = pair({ echo: () => 1, "edit/write": () => 1, "edit/stop": () => 1 }, { bounds, now: () => 1000 });
  // A stream that used its whole window...
  const writes = await Promise.all([1, 2, 3, 4, 5].map(() => call("edit/write")));
  assert.equal(writes.filter((a) => a.error).length, 0);
  // ...leaves the add-in's other calls their own budget,
  const others = await Promise.all([1, 2, 3].map(() => call("echo")));
  assert.equal(others.filter((a) => a.error).length, 0);
  // and the stream itself is still bounded.
  assert.equal((await call("edit/stop")).error?.data?.code, "rate-limited");
  close();
});

// ---- A running plugin, over a real port, with a fake frame ------------------------------

function fakeDocument() {
  const frames = [];
  globalThis.document = {
    createElement(tag) {
      assert.equal(tag, "iframe");
      const listeners = [];
      const frame = {
        attributes: new Map(),
        dataset: {},
        setAttribute(k, v) { frame.attributes.set(k, v); },
        addEventListener(type, fn) { if (type === "load") listeners.push(fn); },
        remove() { frame.removed = true; },
        load() { for (const fn of listeners) fn(); },
        contentWindow: { postMessage(message, target, ports) { frame.connected = { message, target, port: ports[0] }; } },
      };
      frames.push(frame);
      return frame;
    },
  };
  return frames;
}

function client(port) {
  let next = 1;
  const waiting = new Map();
  port.onmessage = ({ data }) => {
    if (waiting.has(data.id)) {
      waiting.get(data.id)(data);
      waiting.delete(data.id);
    }
  };
  return (method, params) =>
    new Promise((resolve) => {
      const id = next++;
      waiting.set(id, resolve);
      port.postMessage({ jsonrpc: "2.0", id, method, params });
    });
}

const vocabulary = {
  schema: { oneOf: [{ properties: { op: { const: "comment" }, quote: { type: "string" } }, required: ["op", "quote"] }, { properties: { op: { const: "replace_text" } } }] },
  commands: { commands: [{ id: "comment", trackable: "not-applicable" }, { id: "replace_text", trackable: "yes" }] },
};

async function running({ permissions = ["read", "suggest"], granted = permissions, access = { access: "review", direct: false } } = {}) {
  const frames = fakeDocument();
  const calls = { preview: [], apply: 0, read: [], execute: [] };
  const { manifest } = checkManifest({ ...base(), permissions }, where);
  const plugin = startPlugin({
    manifest,
    granted: () => granted,
    container: { append() {} },
    env: {
      access: () => access,
      ai: {
        read: (r) => (calls.read.push(r), { ok: true, value: { blocks: [] } }),
        preview: (plan) => (calls.preview.push(plan), { ok: true, previewId: 7, wouldApply: true, preview: { redline: [] } }),
        apply: () => (calls.apply += 1),
      },
      execute: (id) => (calls.execute.push(id), { ok: true }),
      contractRow: (id) => commandContract(id),
      commandIds: () => ["format.bold", "view.outline", "file.save"],
      providers: { describe: () => ({ available: true, kind: "openai", endpoint: "api.example" }), complete: async () => ({ ok: true, response: { choices: [] } }) },
      loadVocabulary: async () => vocabulary,
      selection: () => ({ hasRange: true, text: "hello", anchor: { node: "p1", offset: 0 }, focus: { node: "p1", offset: 5 } }),
      notify: () => {},
      panel: () => {},
      locale: () => "en",
      direction: () => "ltr",
      theme: () => "light",
      storage: { get: () => null, set: () => {} },
      fetchEntry: async () => "<!doctype html><p>plugin</p>",
      onClosed: (reason) => (calls.closed = reason),
    },
  });
  await new Promise((r) => setTimeout(r, 0));
  const frame = frames[0];
  frame.load();
  assert.deepEqual(frame.connected.message, CONNECT_MESSAGE);
  const port = frame.connected.port;
  opened.push(() => {
    plugin.close("test");
    port.close();
  });
  return { plugin, frame, calls, call: client(port) };
}

test("a plugin's frame is sandboxed to scripts only, from srcdoc carrying the CSP", async () => {
  const { frame, plugin } = await running();
  assert.equal(frame.attributes.get("sandbox"), "allow-scripts");
  assert.match(frame.srcdoc, /Content-Security-Policy/);
  plugin.close("test");
});

test("tools/list is the WebMCP tool table for the plugin's grant", async () => {
  const { call, plugin } = await running();
  const names = (await call("tools/list")).result.tools.map((tool) => tool.name);
  assert.ok(names.includes("read_blocks") && names.includes("propose_edits"));
  assert.equal(names.includes("review_tracked_changes"), false, "a plugin never decides tracked changes");
  plugin.close("test");
});

test("an edit goes to the private preview and never to apply", async () => {
  const { call, calls, plugin } = await running();
  const out = (await call("tools/call", { name: "propose_edits", arguments: { base_revision: 3, edits: [{ op: "replace_text" }] } })).result;
  assert.equal(out.status, "awaiting-person");
  assert.equal(calls.preview.length, 1);
  assert.equal(calls.apply, 0);
  plugin.close("test");
});

test("without read a plugin may not list tools, read the selection or edit", async () => {
  const { call, plugin } = await running({ permissions: ["read", "suggest"], granted: ["suggest"] });
  for (const [method, params] of [["tools/list"], ["selection/get"], ["tools/call", { name: "read_blocks", arguments: {} }]]) {
    const answer = await call(method, params);
    assert.equal(answer.error?.data?.code, "permission-denied", method);
  }
  plugin.close("test");
});

test("a readonly mount narrows a suggest plugin to reads", async () => {
  const { call, calls, plugin } = await running({ access: { access: "read", direct: false } });
  const names = (await call("tools/list")).result.tools.map((tool) => tool.name);
  assert.equal(names.includes("propose_edits"), false);
  const refused = await call("tools/call", { name: "propose_edits", arguments: { base_revision: 1, edits: [] } });
  assert.equal(refused.error.data.code, "permission-denied");
  assert.equal(calls.preview.length, 0);
  plugin.close("test");
});

test("commands by id: a suggest plugin may not run format.bold or file.save, may run view.outline", async () => {
  const { call, calls, plugin } = await running();
  assert.deepEqual((await call("commands/list")).result.commands, ["view.outline"]);
  assert.equal((await call("commands/run", { id: "format.bold" })).error.data.code, "permission-denied");
  assert.equal((await call("commands/run", { id: "file.save" })).error.data.code, "permission-denied");
  assert.equal((await call("commands/run", { id: "view.outline" })).result.ok, true);
  assert.deepEqual(calls.execute, ["view.outline"]);
  plugin.close("test");
});

test("ai/complete needs the ai permission; initialize never reveals more than the disclosure", async () => {
  const { call, plugin } = await running();
  assert.equal((await call("ai/complete", { request: { messages: [] } })).error.data.code, "permission-denied");
  const info = (await call("initialize")).result;
  assert.deepEqual(info.provider, { available: false });
  plugin.close("test");
  const withAi = await running({ permissions: ["read", "ai"] });
  const info2 = (await withAi.call("initialize")).result;
  // Plus what shapes the request, never who: whether the model runs on this
  // computer (a longer first-byte wait) and its context window, if known.
  assert.deepEqual(info2.provider, { available: true, kind: "openai", endpoint: "api.example", local: false, contextTokens: null });
  assert.equal(JSON.stringify(info2).includes("apiKey"), false);
  withAi.plugin.close("test");
});

test("a frame that navigates itself is closed and reported", async () => {
  const { frame, calls } = await running();
  frame.load();
  assert.equal(calls.closed, "navigated");
  assert.equal(frame.removed, true);
});
