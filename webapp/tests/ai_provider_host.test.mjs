// The host's model over the host contract (`docs/183` §3.8): in process as
// `window.opendoc.aiProvider`, and over `postMessage` as the `ai-provider` event
// and the `ai-provider.answer` request — the same JSON both ways, and no key.
import assert from "node:assert/strict";
import test from "node:test";

import { HOST_EVENT_NAMES, PROTOCOL } from "../src/host_contract.mjs";
import { createHostSession } from "../src/host_session.mjs";
import { attachHostBridge } from "../src/host_bridge.mjs";
import { createHostClient } from "../src/host_client.mjs";
import { resolveCapabilities, hostConfig } from "../src/capabilities.mjs";
import { createAiProviders } from "../src/ai_provider_channel.mjs";

/** A host window and an editor window wired as each other's peer (same origin). */
function wire() {
  const ORIGIN = "https://docs.example";
  const make = () => {
    const win = {
      location: { origin: ORIGIN, search: "" },
      listeners: new Set(),
      addEventListener(type, fn) { if (type === "message") win.listeners.add(fn); },
      removeEventListener(type, fn) { win.listeners.delete(fn); },
      postMessage(data, targetOrigin) {
        if (targetOrigin !== ORIGIN) return;
        for (const fn of [...win.listeners]) fn({ data, origin: ORIGIN, source: win.peer });
      },
    };
    return win;
  };
  const host = make(), editor = make();
  host.peer = editor; editor.peer = host; editor.parent = host;
  return { host, editor, ORIGIN };
}

const session = () => createHostSession({ capabilities: resolveCapabilities({ mode: "edit", framed: true }), registry: () => [] });
const ask = { model: "", messages: [{ role: "user", content: "Rewrite: hello" }] };
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

test("the contract declares the event and the answer verb", () => {
  assert.ok(HOST_EVENT_NAMES.includes("ai-provider"));
  assert.ok(PROTOCOL.requests.includes("ai-provider.answer"));
});

test("hostConfig reads ?aiProvider and ?plugins", () => {
  const view = (search) => ({ location: { search }, self: 1, top: 1 });
  assert.equal(hostConfig(view("?aiProvider=host")).aiProvider, "host");
  assert.equal(hostConfig(view("?aiprovider=off")).aiProvider, "off", "names are matched without regard to case");
  assert.equal(hostConfig(view("")).aiProvider, null);
  assert.equal(hostConfig(view("?plugins=none")).plugins, "none");
});

test("in process: the host's aiProvider answers, and the editor never sees a key", async () => {
  const s = session();
  let got = null;
  s.aiProvider = async (request, { purpose, onChunk }) => {
    got = { request, purpose };
    onChunk({ choices: [{ index: 0, delta: { content: "Hi" } }] });
    return { choices: [{ message: { role: "assistant", content: "Hi there" }, finish_reason: "stop" }] };
  };
  const providers = createAiProviders({ framed: true, hostFn: () => s.aiProvider, remote: s.aiProviderChannel });
  assert.equal(providers.describe().kind, "host");
  const chunks = [];
  const out = await providers.complete(ask, { purpose: "rewrite", onChunk: (c) => chunks.push(c) });
  assert.equal(out.response.choices[0].message.content, "Hi there");
  assert.equal(got.purpose, "rewrite");
  assert.equal(got.request.max_completion_tokens, 4096, "the body is the checked contract request");
  assert.equal(chunks.length, 1);
  s.aiProvider = null;
  assert.equal(providers.available(), false, "taking the property away takes AI away in a frame");
});

test("over postMessage: request out, chunks and result back, the same answer as in process", async () => {
  const s = session();
  const { host, editor, ORIGIN } = wire();
  attachHostBridge({ session: s, view: editor });
  const client = createHostClient({ frame: { contentWindow: editor }, editorOrigin: ORIGIN, view: host, timeout: 1000 });
  const seen = [];
  const stop = client.serveAiProvider(async (request, { onChunk, purpose }) => {
    seen.push({ request, purpose });
    onChunk({ choices: [{ index: 0, delta: { content: "Bon" } }] });
    onChunk({ choices: [{ index: 0, delta: { content: "jour" } }] });
    return { choices: [{ message: { role: "assistant", content: "Bonjour" }, finish_reason: "stop" }], usage: { prompt_tokens: 3, completion_tokens: 1, total_tokens: 4 } };
  });
  const providers = createAiProviders({ framed: true, config: { aiProvider: "host" }, remote: s.aiProviderChannel });
  assert.equal(providers.describe().kind, "host");
  const chunks = [];
  const out = await providers.complete(ask, { purpose: "translate", onChunk: (c) => chunks.push(c.choices[0].delta.content) });
  assert.equal(out.ok, true, JSON.stringify(out));
  assert.equal(out.response.choices[0].message.content, "Bonjour");
  assert.equal(out.response.usage.total_tokens, 4);
  assert.deepEqual(chunks, ["Bon", "jour"]);
  assert.equal(seen[0].purpose, "translate");
  assert.equal(seen[0].request.messages[0].content, "Rewrite: hello");
  stop();
  client.dispose();
});

test("over postMessage: Stop reaches the host's call as an abort", async () => {
  const s = session();
  const { host, editor, ORIGIN } = wire();
  attachHostBridge({ session: s, view: editor });
  const client = createHostClient({ frame: { contentWindow: editor }, editorOrigin: ORIGIN, view: host, timeout: 1000 });
  let hostSignal = null;
  client.serveAiProvider((request, { signal }) => {
    hostSignal = signal;
    return new Promise(() => {}); // never answers on its own
  });
  const providers = createAiProviders({ framed: true, config: { aiProvider: "host" }, remote: s.aiProviderChannel });
  const controller = new AbortController();
  const pending = providers.complete(ask, { signal: controller.signal });
  await settle();
  controller.abort();
  const out = await pending;
  assert.equal(out.error.code, "cancelled");
  await settle();
  assert.equal(hostSignal.aborted, true, "the host's own request was cancelled too");
  assert.equal(s.aiProviderChannel.inFlight(), 0);
  client.dispose();
});

test("over postMessage: a host error object comes back as host-refused", async () => {
  const s = session();
  const { host, editor, ORIGIN } = wire();
  attachHostBridge({ session: s, view: editor });
  const client = createHostClient({ frame: { contentWindow: editor }, editorOrigin: ORIGIN, view: host, timeout: 1000 });
  client.serveAiProvider(async () => ({ error: { message: "Your plan does not include AI.", code: "plan" } }));
  const providers = createAiProviders({ framed: true, config: { aiProvider: "host" }, remote: s.aiProviderChannel });
  const out = await providers.complete(ask);
  assert.equal(out.error.code, "host-refused");
  assert.equal(out.error.message, "Your plan does not include AI.");
  client.dispose();
});

test("without ?aiProvider=host a framed editor asks nobody, even with the channel present", async () => {
  const s = session();
  const events = [];
  s.on("ai-provider", (e) => events.push(e));
  const providers = createAiProviders({ framed: true, remote: s.aiProviderChannel });
  assert.equal((await providers.complete(ask)).error.code, "provider-unconfigured");
  assert.equal(events.length, 0);
});
