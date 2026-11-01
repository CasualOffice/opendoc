// The AI provider contract and its adapters (`docs/183` §3), against a LOCAL MOCK
// SERVER: an `node:http` server on 127.0.0.1 speaking OpenAI Chat Completions and
// Anthropic Messages, JSON and SSE. No real endpoint and no real key, ever.
import assert from "node:assert/strict";
import test from "node:test";
import { createServer } from "node:http";
import { readFileSync } from "node:fs";

import {
  AI_ERROR_CODES,
  FINISH_REASONS,
  REQUEST_FIELDS,
  checkRequest,
  checkResponse,
  createBudget,
  endpointAllowed,
  estimateTokens,
} from "../src/ai_provider.mjs";
import {
  anthropicAdapter,
  fromSamplingResult,
  hostAdapter,
  mcpSamplingAdapter,
  openAiAdapter,
  parseSse,
  toAnthropicRequest,
  toSamplingParams,
} from "../src/ai_provider_adapters.mjs";
import { createAiProviders, createHostProviderChannel } from "../src/ai_provider_channel.mjs";

const schema = (name) => JSON.parse(readFileSync(new URL(`../schemas/${name}.schema.json`, import.meta.url), "utf8"));

// ---- The mock server ---------------------------------------------------------

const seen = [];
function sse(res, records) {
  res.writeHead(200, { "content-type": "text/event-stream" });
  for (const [event, data] of records) res.write(`${event ? `event: ${event}\n` : ""}data: ${typeof data === "string" ? data : JSON.stringify(data)}\n\n`);
  res.end();
}
const lastUser = (body) => {
  const msgs = body.messages ?? [];
  const m = [...msgs].reverse().find((x) => x.role === "user");
  return typeof m?.content === "string" ? m.content : (m?.content?.[0]?.text ?? "");
};

const server = createServer((req, res) => {
  let raw = "";
  req.on("data", (c) => (raw += c));
  req.on("end", () => {
    const body = raw ? JSON.parse(raw) : {};
    seen.push({ url: req.url, headers: req.headers, body });
    const prompt = lastUser(body);
    if (req.url.startsWith("/v1/chat/completions")) {
      if (req.headers.authorization === "Bearer bad") {
        res.writeHead(401, { "content-type": "application/json" });
        return res.end(JSON.stringify({ error: { message: "Incorrect API key", type: "invalid_request_error", code: "invalid_api_key" } }));
      }
      if (prompt === "rate") {
        res.writeHead(429, { "content-type": "application/json", "retry-after": "7" });
        return res.end(JSON.stringify({ error: { message: "slow down", type: "requests", code: "rate_limit_exceeded" } }));
      }
      if (prompt === "ctx") {
        res.writeHead(400, { "content-type": "application/json" });
        return res.end(JSON.stringify({ error: { message: "too long", type: "invalid_request_error", code: "context_length_exceeded" } }));
      }
      if (prompt === "hang") return; // never answers: cancellation and timeouts
      if (prompt === "drip") {
        // First chunk, then silence: Stop mid-stream must keep the partial text.
        res.writeHead(200, { "content-type": "text/event-stream" });
        res.write(`data: ${JSON.stringify({ id: "c1", model: "m", choices: [{ index: 0, delta: { content: "Par" }, finish_reason: null }] })}\n\n`);
        return;
      }
      if (body.stream) {
        return sse(res, [
          [null, { id: "c1", model: body.model, choices: [{ index: 0, delta: { role: "assistant" }, finish_reason: null }] }],
          [null, { id: "c1", model: body.model, choices: [{ index: 0, delta: { content: "Hel" }, finish_reason: null }] }],
          [null, "not json"],
          [null, { id: "c1", model: body.model, choices: [{ index: 0, delta: { content: "lo" }, finish_reason: "stop" }] }],
          [null, { id: "c1", model: body.model, choices: [], usage: { prompt_tokens: 5, completion_tokens: 2, total_tokens: 7 } }],
          [null, "[DONE]"],
        ]);
      }
      res.writeHead(200, { "content-type": "application/json" });
      return res.end(JSON.stringify({
        id: "c0", object: "chat.completion", created: 1, model: body.model,
        choices: [{ index: 0, message: { role: "assistant", content: `echo:${prompt}` }, finish_reason: "stop" }],
        usage: { prompt_tokens: 3, completion_tokens: 2, total_tokens: 5 },
        extra_vendor_field: "dropped",
      }));
    }
    if (req.url === "/v1/messages") {
      if (req.headers["x-api-key"] !== "sk-ant-test" || req.headers["anthropic-version"] !== "2023-06-01" || req.headers["anthropic-dangerous-direct-browser-access"] !== "true") {
        res.writeHead(401, { "content-type": "application/json" });
        return res.end(JSON.stringify({ type: "error", error: { type: "authentication_error", message: "bad headers" } }));
      }
      if (prompt === "overloaded") {
        res.writeHead(529, { "content-type": "application/json" });
        return res.end(JSON.stringify({ type: "error", error: { type: "overloaded_error", message: "Overloaded" } }));
      }
      if (body.stream) {
        return sse(res, [
          ["message_start", { type: "message_start", message: { id: "msg_1", model: body.model, usage: { input_tokens: 9 } } }],
          ["content_block_start", { type: "content_block_start", index: 0, content_block: { type: "text", text: "" } }],
          ["ping", { type: "ping" }],
          ["content_block_delta", { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: "Bon" } }],
          ["content_block_delta", { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: "jour" } }],
          ["content_block_stop", { type: "content_block_stop", index: 0 }],
          ["message_delta", { type: "message_delta", delta: { stop_reason: "max_tokens" }, usage: { output_tokens: 4 } }],
          ["message_stop", { type: "message_stop" }],
        ]);
      }
      res.writeHead(200, { "content-type": "application/json" });
      return res.end(JSON.stringify({
        id: "msg_0", type: "message", role: "assistant", model: body.model,
        content: [{ type: "text", text: `system=${body.system ?? ""};user=${prompt}` }],
        stop_reason: "end_turn", usage: { input_tokens: 6, output_tokens: 3 },
      }));
    }
    res.writeHead(404);
    res.end();
  });
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const base = `http://127.0.0.1:${server.address().port}`;
test.after(() => server.close());

const user = (text) => ({ role: "user", content: text });
const req = (text, extra = {}) => checkRequest({ model: "mock-1", messages: [user(text)], ...extra }).request;

// ---- The contract --------------------------------------------------------------

test("the schemas and the module name the same request fields, finish reasons and error codes", () => {
  assert.deepEqual(Object.keys(schema("ai-provider-request").properties).sort(), [...REQUEST_FIELDS].sort());
  const response = schema("ai-provider-response");
  assert.deepEqual(response.$defs.normalisedErrorCode.enum, [...AI_ERROR_CODES]);
  assert.deepEqual(response.$defs.response.properties.choices.items.properties.finish_reason.enum, [...FINISH_REASONS]);
});

test("checkRequest refuses a field outside the contract, so nothing provider-specific leaves", () => {
  const out = checkRequest({ model: "m", messages: [user("hi")], logit_bias: { 1: 2 } });
  assert.equal(out.ok, false);
  assert.equal(out.error.code, "bad-request");
  assert.equal(out.error.field, "logit_bias");
});

test("checkRequest always sets max_completion_tokens, clamped to the ceiling", () => {
  assert.equal(checkRequest({ messages: [user("hi")] }, { outputCeiling: 512 }).request.max_completion_tokens, 512);
  assert.equal(checkRequest({ messages: [user("hi")], max_completion_tokens: 9000 }, { outputCeiling: 512 }).request.max_completion_tokens, 512);
  assert.equal(checkRequest({ messages: [user("hi")], max_completion_tokens: 100 }, { outputCeiling: 512 }).request.max_completion_tokens, 100);
});

test("checkRequest refuses over the input budget locally, with the declared estimator", () => {
  const out = checkRequest({ messages: [user("x".repeat(4000))] }, { contextTokens: 100 });
  assert.equal(out.error.code, "context-too-long");
  assert.equal(out.error.estimator, "bytes/4");
  assert.ok(out.error.estimate > 100);
});

test("checkRequest refuses malformed messages", () => {
  assert.equal(checkRequest({ messages: [] }).ok, false);
  assert.equal(checkRequest({ messages: [{ role: "robot", content: "x" }] }).ok, false);
  assert.equal(checkRequest({ messages: [{ role: "tool", content: "x" }] }).ok, false);
  assert.equal(checkRequest({ messages: [{ role: "user", content: [{ type: "image_url", image_url: "x" }] }] }).ok, false);
});

test("the estimate is bytes/4 per message plus framing", () => {
  assert.equal(estimateTokens({ messages: [user("abcdefgh")] }), 4 + 2);
});

test("checkResponse keeps only the contract's fields", () => {
  const out = checkResponse({ id: "a", model: "m", choices: [{ message: { role: "assistant", content: "x" }, finish_reason: "weird" }], vendor: 1 });
  assert.equal(out.ok, true);
  assert.equal(out.response.vendor, undefined);
  assert.equal(out.response.choices[0].finish_reason, "stop");
  assert.equal(checkResponse({ nope: 1 }).error.code, "bad-response");
});

test("only https, or http to this computer, may receive text", () => {
  assert.ok(endpointAllowed("https://api.openai.com/v1"));
  assert.ok(endpointAllowed("http://localhost:11434/v1"));
  assert.ok(endpointAllowed("http://127.0.0.1:1234/v1"));
  assert.equal(endpointAllowed("http://example.com/v1"), false);
  assert.equal(endpointAllowed("https://user:pw@example.com"), false);
  assert.equal(endpointAllowed("javascript:alert(1)"), false);
});

test("the SSE parser joins data lines, skips comments and keeps a last unterminated line", async () => {
  const text = ": hi\nevent: a\ndata: 1\ndata: 2\n\ndata: tail";
  const body = new Response(text).body;
  const out = [];
  for await (const record of parseSse(body)) out.push(record);
  assert.deepEqual(out, [{ event: "a", data: "1\n2" }, { event: "message", data: "tail" }]);
});

// ---- OpenAI-compatible -----------------------------------------------------------

test("openai: a JSON answer, with the bearer key sent and vendor fields dropped", async () => {
  const adapter = openAiAdapter({ baseUrl: `${base}/v1`, apiKey: "sk-test" });
  const out = await adapter.complete(req("ping"));
  assert.equal(out.ok, true);
  assert.equal(out.response.choices[0].message.content, "echo:ping");
  assert.equal(out.response.extra_vendor_field, undefined);
  assert.equal(seen.at(-1).headers.authorization, "Bearer sk-test");
  assert.equal(seen.at(-1).body.max_completion_tokens, 4096);
});

test("openai: a keyless local server gets no Authorization header", async () => {
  await openAiAdapter({ baseUrl: `${base}/v1` }).complete(req("ping"));
  assert.equal(seen.at(-1).headers.authorization, undefined);
});

test("openai: the legacy switch renames max_completion_tokens to max_tokens", async () => {
  await openAiAdapter({ baseUrl: `${base}/v1`, legacyMaxTokens: true }).complete(req("ping"));
  assert.equal(seen.at(-1).body.max_tokens, 4096);
  assert.equal(seen.at(-1).body.max_completion_tokens, undefined);
});

test("openai: a stream renders chunk by chunk and assembles the same answer, usage included", async () => {
  const chunks = [];
  const out = await openAiAdapter({ baseUrl: `${base}/v1` }).complete(req("s", { stream: true }), { onChunk: (c) => chunks.push(c.choices[0]?.delta?.content ?? "") });
  assert.equal(out.ok, true);
  assert.equal(out.response.choices[0].message.content, "Hello");
  assert.deepEqual(chunks.filter(Boolean), ["Hel", "lo"]);
  assert.equal(out.response.usage.total_tokens, 7);
  assert.equal(out.dropped, 1, "the malformed record is counted, never silently passed");
  assert.deepEqual(seen.at(-1).body.stream_options, { include_usage: true });
});

test("openai: 401, 429 and context_length_exceeded normalise to their codes", async () => {
  assert.equal((await openAiAdapter({ baseUrl: `${base}/v1`, apiKey: "bad" }).complete(req("x"))).error.code, "auth-failed");
  const rate = await openAiAdapter({ baseUrl: `${base}/v1` }).complete(req("rate"));
  assert.equal(rate.error.code, "rate-limited");
  assert.equal(rate.error.retryAfter, "7");
  assert.equal((await openAiAdapter({ baseUrl: `${base}/v1` }).complete(req("ctx"))).error.code, "context-too-long");
});

test("openai: Stop mid-stream resolves cancelled and keeps the text that had arrived", async () => {
  const controller = new AbortController();
  const out = await openAiAdapter({ baseUrl: `${base}/v1` }).complete(req("drip", { stream: true }), {
    signal: controller.signal,
    onChunk: () => controller.abort(),
  });
  assert.equal(out.error.code, "cancelled");
  assert.equal(out.error.partial, "Par");
});

test("openai: no first byte within the deadline is a timeout, not a hang", async () => {
  const out = await openAiAdapter({ baseUrl: `${base}/v1`, timers: { firstByteMs: 50, idleMs: 50 } }).complete(req("hang"));
  assert.equal(out.error.code, "timeout");
});

test("openai: an unreachable endpoint is `network` and names CORS", async () => {
  const out = await openAiAdapter({ baseUrl: "http://127.0.0.1:9/v1" }).complete(req("x"));
  assert.equal(out.error.code, "network");
  assert.match(out.error.message, /CORS/);
});

test("openai: Azure sends api-key and api-version", async () => {
  await openAiAdapter({ baseUrl: `${base}/v1`, apiKey: "az", azure: true, apiVersion: "2024-10-21" }).complete(req("ping"));
  assert.equal(seen.at(-1).headers["api-key"], "az");
  assert.match(seen.at(-1).url, /api-version=2024-10-21/);
});

// ---- Anthropic ---------------------------------------------------------------------

test("anthropic: system messages lift into `system`, roles merge to alternate, max_tokens is required", () => {
  const body = toAnthropicRequest(checkRequest({
    model: "claude-test",
    messages: [{ role: "system", content: "Be brief." }, user("a"), user("b"), { role: "assistant", content: "c" }],
    stop: ["END"],
    temperature: 1.5,
  }).request);
  assert.equal(body.system, "Be brief.");
  assert.equal(body.messages.length, 2);
  assert.equal(body.messages[0].content.length, 2);
  assert.equal(body.max_tokens, 4096);
  assert.deepEqual(body.stop_sequences, ["END"]);
  assert.equal(body.temperature, 1, "Anthropic's temperature tops out at 1");
});

test("anthropic: tool calls and tool results map to tool_use and tool_result", () => {
  const body = toAnthropicRequest(checkRequest({
    messages: [
      user("q"),
      { role: "assistant", content: null, tool_calls: [{ id: "t1", type: "function", function: { name: "read_blocks", arguments: "{\"start\":\"p1\"}" } }] },
      { role: "tool", tool_call_id: "t1", content: "text" },
    ],
    tools: [{ type: "function", function: { name: "read_blocks", parameters: { type: "object" } } }],
    tool_choice: "required",
  }).request);
  assert.deepEqual(body.messages[1].content[0], { type: "tool_use", id: "t1", name: "read_blocks", input: { start: "p1" } });
  assert.equal(body.messages[2].content[0].type, "tool_result");
  assert.deepEqual(body.tool_choice, { type: "any" });
  assert.equal(body.tools[0].input_schema.type, "object");
});

test("anthropic: a JSON answer, with the three required headers sent", async () => {
  const adapter = anthropicAdapter({ baseUrl: base, apiKey: "sk-ant-test" });
  const out = await adapter.complete(checkRequest({ model: "claude-test", messages: [{ role: "system", content: "S" }, user("hi")] }).request);
  assert.equal(out.ok, true, JSON.stringify(out));
  assert.equal(out.response.choices[0].message.content, "system=S;user=hi");
  assert.equal(out.response.usage.total_tokens, 9);
});

test("anthropic: a stream maps text deltas, stop_reason and usage", async () => {
  const chunks = [];
  const out = await anthropicAdapter({ baseUrl: base, apiKey: "sk-ant-test" }).complete(req("s", { stream: true }), { onChunk: (c) => chunks.push(c) });
  assert.equal(out.response.choices[0].message.content, "Bonjour");
  assert.equal(out.response.choices[0].finish_reason, "length");
  assert.equal(out.response.usage.prompt_tokens, 9);
  assert.equal(out.response.usage.completion_tokens, 4);
  assert.ok(chunks.length >= 3);
});

test("anthropic: overloaded is a server error; wrong headers are auth-failed", async () => {
  assert.equal((await anthropicAdapter({ baseUrl: base, apiKey: "sk-ant-test" }).complete(req("overloaded"))).error.code, "server-error");
  assert.equal((await anthropicAdapter({ baseUrl: base, apiKey: "wrong" }).complete(req("x"))).error.code, "auth-failed");
});

// ---- Host and MCP sampling -------------------------------------------------------------

test("host: the body reaches the callback unchanged, chunks stream, the answer is checked", async () => {
  let got = null;
  const chunks = [];
  const adapter = hostAdapter(async (request, { onChunk, purpose }) => {
    got = { request, purpose };
    onChunk({ choices: [{ index: 0, delta: { content: "A" } }] });
    return { choices: [{ message: { role: "assistant", content: "AB" }, finish_reason: "stop" }] };
  });
  const request = req("hello");
  const out = await adapter.complete(request, { purpose: "rewrite", onChunk: (c) => chunks.push(c) });
  assert.deepEqual(got.request, request);
  assert.equal(got.purpose, "rewrite");
  assert.equal(out.response.choices[0].message.content, "AB");
  assert.equal(chunks.length, 1);
});

test("host: an OpenAI error object from the host is host-refused unless its status says more", async () => {
  const out = await hostAdapter(async () => ({ error: { message: "policy", code: "blocked" } })).complete(req("x"));
  assert.equal(out.error.code, "host-refused");
  const rate = await hostAdapter(async () => ({ error: { message: "slow" }, status: 429 })).complete(req("x"));
  assert.equal(rate.error.code, "rate-limited");
});

test("host channel: request out as an event, chunks and the result back by qid, cancel on Stop", async () => {
  const events = [];
  const channel = createHostProviderChannel({ emit: (name, detail) => events.push({ name, detail }) });
  const adapter = hostAdapter((request, options) => channel.ask(request, options));
  const pending = adapter.complete(req("hi"), { purpose: "chat" });
  const { qid, kind, request } = events[0].detail;
  assert.equal(events[0].name, "ai-provider");
  assert.equal(kind, "request");
  assert.equal(request.messages[0].content, "hi");
  assert.equal(channel.answer({ qid, chunk: { choices: [{ index: 0, delta: { content: "y" } }] } }), true);
  assert.equal(channel.answer({ qid, response: { choices: [{ message: { role: "assistant", content: "yes" } }] } }), true);
  assert.equal((await pending).response.choices[0].message.content, "yes");
  assert.equal(channel.answer({ qid, response: {} }), false, "a late answer is ignored");

  const controller = new AbortController();
  const cancelled = adapter.complete(req("hi"), { signal: controller.signal });
  controller.abort();
  assert.equal((await cancelled).error.code, "cancelled");
  assert.equal(events.at(-1).detail.kind, "cancel");
  assert.equal(channel.inFlight(), 0);
});

test("host channel: a host that never answers times out and is told to cancel", async () => {
  const events = [];
  const channel = createHostProviderChannel({ emit: (name, detail) => events.push(detail), timeoutMs: 20 });
  const out = await hostAdapter((r, o) => channel.ask(r, o)).complete(req("x"));
  assert.equal(out.error.code, "timeout");
  assert.equal(events.at(-1).kind, "cancel");
});

test("mcp sampling: the request maps to sampling/createMessage and back", async () => {
  const params = toSamplingParams(checkRequest({ model: "claude", messages: [{ role: "system", content: "S" }, user("q")], stop: ["Z"] }).request);
  assert.deepEqual(params.messages, [{ role: "user", content: { type: "text", text: "q" } }]);
  assert.equal(params.systemPrompt, "S");
  assert.equal(params.maxTokens, 4096);
  assert.deepEqual(params.stopSequences, ["Z"]);
  assert.deepEqual(params.modelPreferences, { hints: [{ name: "claude" }] });
  const back = fromSamplingResult({ role: "assistant", content: { type: "text", text: "a" }, model: "x", stopReason: "maxTokens" });
  assert.equal(back.response.choices[0].finish_reason, "length");
  let method = null;
  const out = await mcpSamplingAdapter(async (m) => {
    method = m;
    return { role: "assistant", content: { type: "text", text: "lent" }, model: "host-model", stopReason: "endTurn" };
  }).complete(req("q"));
  assert.equal(method, "sampling/createMessage");
  assert.equal(out.response.choices[0].message.content, "lent");
});

// ---- Resolution, budgets, audit ------------------------------------------------------------

function countingFetch() {
  const calls = [];
  const fetchImpl = (...args) => {
    calls.push(args[0]);
    return fetch(...args);
  };
  return { calls, fetchImpl };
}

test("no provider configured: nothing is available and NO network call is made", async () => {
  const { calls, fetchImpl } = countingFetch();
  const providers = createAiProviders({ settings: () => null, fetchImpl });
  assert.equal(providers.available(), false);
  assert.equal(providers.unavailableReason(), "unconfigured");
  const out = await providers.complete({ messages: [user("secret document text")] });
  assert.equal(out.error.code, "provider-unconfigured");
  assert.equal(calls.length, 0);
});

test("a framed editor never uses BYOK, even with a key stored", async () => {
  const { calls, fetchImpl } = countingFetch();
  const providers = createAiProviders({ framed: true, settings: () => ({ protocol: "openai", baseUrl: `${base}/v1`, model: "m" }), fetchImpl });
  assert.equal(providers.available(), false);
  assert.equal(providers.unavailableReason(), "framed");
  await providers.complete({ messages: [user("x")] });
  assert.equal(calls.length, 0);
});

test("?aiProvider=off withdraws every provider, the host's included", async () => {
  const providers = createAiProviders({ config: { aiProvider: "off" }, settings: () => ({ baseUrl: `${base}/v1` }) });
  providers.hostProvider = async () => ({ choices: [{ message: { role: "assistant", content: "x" } }] });
  assert.equal((await providers.complete({ messages: [user("x")] })).error.code, "provider-disabled");
});

test("the in-process host provider wins over BYOK, and the disclosure says which", async () => {
  const { calls, fetchImpl } = countingFetch();
  const providers = createAiProviders({ settings: () => ({ protocol: "openai", baseUrl: `${base}/v1`, model: "m" }), fetchImpl });
  assert.equal(providers.describe().kind, "openai");
  assert.equal(providers.describe().endpoint, `127.0.0.1:${server.address().port}`);
  providers.hostProvider = async () => ({ choices: [{ message: { role: "assistant", content: "host" } }] });
  assert.equal(providers.describe().kind, "host");
  const out = await providers.complete({ messages: [user("x")] });
  assert.equal(out.response.choices[0].message.content, "host");
  assert.equal(calls.length, 0);
});

test("BYOK: the stored model is used and the call goes to the stored endpoint", async () => {
  const providers = createAiProviders({ settings: () => ({ protocol: "openai", baseUrl: `${base}/v1`, model: "stored-model", apiKey: "k" }) });
  const out = await providers.complete({ messages: [user("ping")] }, { purpose: "test" });
  assert.equal(out.ok, true);
  assert.equal(seen.at(-1).body.model, "stored-model");
});

test("a spent budget sends nothing", async () => {
  const { calls, fetchImpl } = countingFetch();
  const providers = createAiProviders({ settings: () => ({ baseUrl: `${base}/v1` }), budget: { requests: 1 }, fetchImpl });
  assert.equal((await providers.complete({ messages: [user("ping")] })).ok, true);
  assert.equal((await providers.complete({ messages: [user("ping")] })).error.code, "over-budget");
  assert.equal(calls.length, 1);
});

test("createBudget charges the estimate plus the output ceiling, then settles to real usage", () => {
  const budget = createBudget({ tokens: 1000 });
  const charge = budget.charge(100, 400);
  assert.equal(budget.remaining().tokens, 500);
  budget.settle(charge.projected, { total_tokens: 150 });
  assert.equal(budget.remaining().tokens, 850);
  assert.equal(budget.charge(500, 400).error.code, "over-budget");
});

test("the audit records purpose, endpoint, usage and code, and never the text", async () => {
  const records = [];
  const providers = createAiProviders({ settings: () => ({ baseUrl: `${base}/v1`, model: "m" }), onAudit: (r) => records.push(r) });
  await providers.complete({ messages: [user("THE CONFIDENTIAL PARAGRAPH")] }, { purpose: "rewrite" });
  assert.equal(records.length, 1);
  assert.equal(records[0].purpose, "rewrite");
  assert.equal(records[0].code, "ok");
  assert.equal(records[0].usage.total_tokens, 5);
  assert.doesNotMatch(JSON.stringify(providers.audit()), /CONFIDENTIAL|echo:/);
});
