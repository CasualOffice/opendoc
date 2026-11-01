// SPDX-License-Identifier: Apache-2.0
// The four adapters behind the AI provider contract (`docs/183` §3.5).
//
// Each adapter is `{kind, endpoint, complete(request, {signal, onChunk, purpose})}`
// and resolves — never rejects — to `{ok: true, response}` or `{ok: false,
// error}` in the contract's shapes (`ai_provider.mjs`). `request` has already
// passed `checkRequest`.
//
//   openai        POST {baseUrl}/chat/completions — the wire format itself
//   anthropic     POST {baseUrl}/v1/messages, mapped both ways
//                 (<https://docs.anthropic.com/en/api/messages>,
//                 <https://docs.anthropic.com/en/api/messages-streaming>)
//   host          the host's own `aiProvider` callback, in process or over
//                 `postMessage`: the body is passed as is
//   mcp-sampling  `sampling/createMessage` through a requester an MCP host
//                 supplies (<https://modelcontextprotocol.io/specification/2026-07-28/client/sampling>).
//                 DEPRECATED in that revision (SEP-2577); a mapping behind the
//                 contract that leaves with the feature (`docs/183` §3.7).
//
// Streaming is SSE over `fetch`; the parser is the WHATWG event-stream format
// (<https://html.spec.whatwg.org/multipage/server-sent-events.html#event-stream-interpretation>),
// reduced to the `event` and `data` fields both providers use.
//
// Complexity: O(response size) per call. No adapter reads the document.
import {
  LIMITS,
  aiError,
  checkChunk,
  checkResponse,
  codeForStatus,
  createAssembler,
  endpointAllowed,
  endpointHost,
  fromOpenAiError,
} from "./ai_provider.mjs";

/** The Anthropic API version this adapter speaks. */
export const ANTHROPIC_VERSION = "2023-06-01";

/**
 * Parses an SSE byte stream into `{event, data}` records. A record ends at a
 * blank line; `data:` lines join with "\n"; comments (`:`) are skipped.
 *
 * O(stream).
 *
 * @param {ReadableStream<Uint8Array>} body
 * @returns {AsyncGenerator<{event: string, data: string}>}
 */
export async function* parseSse(body) {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let event = "";
  let data = [];
  const flush = function* () {
    if (data.length) yield { event: event || "message", data: data.join("\n") };
    event = "";
    data = [];
  };
  try {
    for (;;) {
      const { value, done } = await reader.read();
      buffer += done ? decoder.decode() : decoder.decode(value, { stream: true });
      let newline;
      while ((newline = buffer.search(/\r\n|\r|\n/)) >= 0) {
        const line = buffer.slice(0, newline);
        buffer = buffer.slice(newline + (buffer.startsWith("\r\n", newline) ? 2 : 1));
        if (line === "") {
          yield* flush();
          continue;
        }
        if (line.startsWith(":")) continue;
        const colon = line.indexOf(":");
        const field = colon < 0 ? line : line.slice(0, colon);
        let value_ = colon < 0 ? "" : line.slice(colon + 1);
        if (value_.startsWith(" ")) value_ = value_.slice(1);
        if (field === "event") event = value_;
        else if (field === "data") data.push(value_);
      }
      if (done) {
        if (buffer) {
          // A final line without its newline still counts.
          const colon = buffer.indexOf(":");
          if (colon >= 0 && buffer.slice(0, colon) === "data") data.push(buffer.slice(colon + 1).replace(/^ /, ""));
        }
        yield* flush();
        return;
      }
    }
  } finally {
    reader.releaseLock?.();
  }
}

/**
 * One signal that aborts on the caller's signal, on no first byte within
 * `firstByteMs`, and on no progress within `idleMs`. `progress()` resets the
 * idle clock; `reason()` says which of the three fired.
 */
function watchdog(signal, { firstByteMs = LIMITS.firstByteMs, idleMs = LIMITS.idleMs, setTimer = setTimeout, clearTimer = clearTimeout } = {}) {
  const controller = new AbortController();
  let why = null;
  let timer = setTimer(() => {
    why = "timeout";
    controller.abort();
  }, firstByteMs);
  const onAbort = () => {
    why ??= "cancelled";
    controller.abort();
  };
  if (signal?.aborted) onAbort();
  signal?.addEventListener?.("abort", onAbort, { once: true });
  return {
    signal: controller.signal,
    progress() {
      clearTimer(timer);
      timer = setTimer(() => {
        why = "timeout";
        controller.abort();
      }, idleMs);
    },
    reason: () => why,
    done() {
      clearTimer(timer);
      signal?.removeEventListener?.("abort", onAbort);
    },
  };
}

/** What a failed `fetch` (or a body read) becomes. */
function transportError(dog, err, assembler) {
  const partial = assembler?.text() ?? "";
  const extra = partial ? { partial } : {};
  if (dog.reason() === "cancelled") return aiError("cancelled", "Stopped.", extra);
  if (dog.reason() === "timeout") return aiError("timeout", "The provider did not answer in time.", extra);
  // A TypeError from fetch is offline, DNS or CORS; the browser does not say
  // which, so the sentence names the usual cause for a local server.
  return aiError("network", "The provider could not be reached. A local server must allow this page's origin (CORS).", { detail: String(err?.message ?? err), ...extra });
}

async function errorFromHttp(response, bodyToError) {
  let parsed = null;
  try {
    parsed = await response.json();
  } catch {
    parsed = null;
  }
  const retry = response.headers?.get?.("retry-after");
  const extra = retry ? { retryAfter: retry } : {};
  if (parsed) return bodyToError(parsed, response.status, extra);
  return aiError(codeForStatus(response.status), `The provider answered ${response.status}.`, { status: response.status, ...extra });
}

/** Joins a base URL and a path without doubling or dropping a slash. */
function joinUrl(base, path) {
  return `${String(base).replace(/\/+$/, "")}/${path.replace(/^\/+/, "")}`;
}

/**
 * Runs one HTTP call: JSON or SSE, through the watchdog, mapping each SSE
 * record with `toChunk` (which returns a contract chunk, `null` to skip, or an
 * error value to stop) and assembling the answer.
 */
async function httpCall({ fetchImpl, url, headers, body, stream, signal, onChunk, toChunk, toResponse, bodyToError, timers }) {
  const dog = watchdog(signal, timers);
  const assembler = createAssembler();
  try {
    let response;
    try {
      response = await fetchImpl(url, { method: "POST", headers, body: JSON.stringify(body), signal: dog.signal });
    } catch (err) {
      return transportError(dog, err);
    }
    dog.progress();
    if (!response.ok) return await errorFromHttp(response, bodyToError);
    if (!stream) {
      let json;
      try {
        json = await response.json();
      } catch (err) {
        return dog.reason() ? transportError(dog, err) : aiError("bad-response", "The answer was not JSON.");
      }
      return toResponse(json);
    }
    if (!response.body) return aiError("bad-response", "The provider sent no stream.");
    let dropped = 0;
    try {
      for await (const record of parseSse(response.body)) {
        dog.progress();
        if (record.data === "[DONE]") break;
        let json;
        try {
          json = JSON.parse(record.data);
        } catch {
          dropped += 1;
          continue;
        }
        const chunk = toChunk(json, record.event);
        if (chunk === null) continue;
        if (chunk?.ok === false) return chunk;
        const checked = checkChunk(chunk);
        if (!checked) {
          dropped += 1;
          continue;
        }
        assembler.push(checked);
        try {
          onChunk?.(checked);
        } catch (err) {
          console.warn("opendoc: an AI chunk listener threw", err);
        }
      }
    } catch (err) {
      return transportError(dog, err, assembler);
    }
    const assembled = assembler.response();
    return assembled.ok && dropped ? Object.freeze({ ...assembled, dropped }) : assembled;
  } finally {
    dog.done();
  }
}

/**
 * The OpenAI-compatible adapter. `azure: true` sends `api-key` instead of a
 * bearer token and appends `api-version`; `legacyMaxTokens: true` renames
 * `max_completion_tokens` to `max_tokens` for servers that only know the old
 * name (some local servers).
 *
 * @param {{baseUrl: string, apiKey?: string, azure?: boolean, apiVersion?: string,
 *   legacyMaxTokens?: boolean, fetchImpl?: typeof fetch, timers?: object}} config
 */
export function openAiAdapter({ baseUrl, apiKey = "", azure = false, apiVersion = "2024-10-21", legacyMaxTokens = false, fetchImpl = globalThis.fetch?.bind(globalThis), timers } = {}) {
  const allowed = endpointAllowed(baseUrl);
  return Object.freeze({
    kind: "openai",
    endpoint: allowed ? endpointHost(baseUrl) : "",
    async complete(request, { signal, onChunk } = {}) {
      if (!allowed) return aiError("provider-unconfigured", "The endpoint must be https, or http to this computer.");
      const body = { ...request };
      if (body.stream) body.stream_options = { include_usage: true };
      if (legacyMaxTokens) {
        body.max_tokens = body.max_completion_tokens;
        delete body.max_completion_tokens;
      }
      let url = joinUrl(baseUrl, "chat/completions");
      const headers = { "content-type": "application/json" };
      if (azure) {
        url += `${url.includes("?") ? "&" : "?"}api-version=${encodeURIComponent(apiVersion)}`;
        if (apiKey) headers["api-key"] = apiKey;
      } else if (apiKey) {
        headers.authorization = `Bearer ${apiKey}`;
      }
      return httpCall({
        fetchImpl,
        url,
        headers,
        body,
        stream: !!body.stream,
        signal,
        onChunk,
        timers,
        toChunk: (json) => (json?.error ? fromOpenAiError(json.error, 0) : json),
        toResponse: checkResponse,
        bodyToError: (json, status, extra) => fromOpenAiError(json?.error ?? json, status, extra),
      });
    },
  });
}

// ---- Anthropic Messages ------------------------------------------------------

/** Anthropic `stop_reason` → Chat Completions `finish_reason`. */
const ANTHROPIC_FINISH = Object.freeze({
  end_turn: "stop",
  stop_sequence: "stop",
  max_tokens: "length",
  tool_use: "tool_calls",
  refusal: "content_filter",
  pause_turn: "stop",
});

const textOf = (content) => (typeof content === "string" ? content : Array.isArray(content) ? content.map((p) => p?.text ?? "").join("") : "");

/**
 * A contract request as an Anthropic Messages body. System messages are lifted
 * into `system`; tool calls and results become `tool_use` / `tool_result`
 * blocks; consecutive same-role messages are merged, because Messages requires
 * the roles to alternate. Pure; O(request).
 */
export function toAnthropicRequest(request) {
  const system = [];
  const messages = [];
  const push = (role, blocks) => {
    const last = messages.at(-1);
    if (last?.role === role) last.content.push(...blocks);
    else messages.push({ role, content: [...blocks] });
  };
  for (const message of request.messages) {
    if (message.role === "system") {
      system.push(textOf(message.content));
      continue;
    }
    if (message.role === "tool") {
      push("user", [{ type: "tool_result", tool_use_id: message.tool_call_id, content: textOf(message.content) }]);
      continue;
    }
    const blocks = [];
    const text = textOf(message.content);
    if (text) blocks.push({ type: "text", text });
    for (const call of message.tool_calls ?? []) {
      let input = {};
      try {
        input = JSON.parse(call.function.arguments || "{}");
      } catch {
        input = {};
      }
      blocks.push({ type: "tool_use", id: call.id, name: call.function.name, input });
    }
    if (blocks.length) push(message.role, blocks);
  }
  const body = { model: request.model, max_tokens: request.max_completion_tokens, messages };
  if (system.length) body.system = system.join("\n\n");
  if (request.temperature !== undefined) body.temperature = Math.min(1, request.temperature);
  if (request.stop) body.stop_sequences = request.stop;
  if (request.stream) body.stream = true;
  if (request.tools) {
    body.tools = request.tools.map((tool) => ({
      name: tool.function.name,
      description: tool.function.description ?? "",
      input_schema: tool.function.parameters ?? { type: "object", properties: {} },
    }));
    if (request.tool_choice === "required") body.tool_choice = { type: "any" };
    else if (request.tool_choice === "none") body.tool_choice = { type: "none" };
    else if (request.tool_choice?.type === "function") body.tool_choice = { type: "tool", name: request.tool_choice.function?.name };
  }
  return body;
}

/** An Anthropic Messages response as a contract response. Pure. */
export function fromAnthropicResponse(json) {
  if (json?.type === "error" || json?.error) return fromAnthropicError(json.error ?? json, 0);
  if (!Array.isArray(json?.content)) return aiError("bad-response", "The answer was not an Anthropic Messages response.");
  const text = json.content.filter((b) => b?.type === "text").map((b) => b.text ?? "").join("");
  const calls = json.content
    .filter((b) => b?.type === "tool_use")
    .map((b) => ({ id: String(b.id), type: "function", function: { name: String(b.name), arguments: JSON.stringify(b.input ?? {}) } }));
  const usage = json.usage
    ? { prompt_tokens: Number(json.usage.input_tokens ?? 0), completion_tokens: Number(json.usage.output_tokens ?? 0) }
    : null;
  return checkResponse({
    id: json.id,
    model: json.model,
    choices: [{ index: 0, message: { role: "assistant", content: text || (calls.length ? null : ""), ...(calls.length ? { tool_calls: calls } : {}) }, finish_reason: ANTHROPIC_FINISH[json.stop_reason] ?? "stop" }],
    ...(usage ? { usage: { ...usage, total_tokens: usage.prompt_tokens + usage.completion_tokens } } : {}),
  });
}

/** An Anthropic error (`{type, message}`) as an `aiError`. `overloaded_error`
 *  is a server error; the rest follow their status. */
export function fromAnthropicError(error, status, extra = {}) {
  const type = String(error?.type ?? "");
  let code = codeForStatus(status);
  if (type === "overloaded_error" || type === "api_error") code = "server-error";
  else if (type === "authentication_error" || type === "permission_error") code = "auth-failed";
  else if (type === "rate_limit_error") code = "rate-limited";
  else if (type === "invalid_request_error" && /prompt is too long|too many tokens/i.test(error?.message ?? "")) code = "context-too-long";
  else if (!status && type === "invalid_request_error") code = "bad-request";
  return aiError(code, String(error?.message ?? ""), { providerCode: type || null, status: status || null, ...extra });
}

/**
 * Maps the Anthropic stream's events onto contract chunks. Stateful (it must
 * remember the message id and which content block is a tool call), so one per
 * call. `message_stop` ends nothing by itself: the stream's end does.
 */
function anthropicStreamMapper() {
  let id = "";
  let model = "";
  let inputTokens = 0;
  /** content block index -> tool-call index */
  const toolIndex = new Map();
  const chunk = (delta, finish = null, usage = null) => ({ id, model, choices: [{ index: 0, delta, finish_reason: finish }], ...(usage ? { usage } : {}) });
  return (json) => {
    switch (json?.type) {
      case "message_start":
        id = String(json.message?.id ?? "");
        model = String(json.message?.model ?? "");
        inputTokens = Number(json.message?.usage?.input_tokens ?? 0);
        return chunk({ role: "assistant" });
      case "content_block_start":
        if (json.content_block?.type === "tool_use") {
          const at = toolIndex.size;
          toolIndex.set(json.index, at);
          return chunk({ tool_calls: [{ index: at, id: String(json.content_block.id), type: "function", function: { name: String(json.content_block.name), arguments: "" } }] });
        }
        return null;
      case "content_block_delta":
        if (json.delta?.type === "text_delta") return chunk({ content: String(json.delta.text ?? "") });
        if (json.delta?.type === "input_json_delta" && toolIndex.has(json.index)) {
          return chunk({ tool_calls: [{ index: toolIndex.get(json.index), function: { arguments: String(json.delta.partial_json ?? "") } }] });
        }
        return null;
      case "message_delta": {
        const output = Number(json.usage?.output_tokens ?? 0);
        return chunk({}, ANTHROPIC_FINISH[json.delta?.stop_reason] ?? null, { prompt_tokens: inputTokens, completion_tokens: output, total_tokens: inputTokens + output });
      }
      case "error":
        return fromAnthropicError(json.error, 0);
      default:
        return null; // ping, content_block_stop, message_stop
    }
  };
}

/**
 * The Anthropic Messages adapter. Sends `anthropic-dangerous-direct-browser-access`,
 * which Anthropic requires for a call made from a browser page.
 *
 * @param {{baseUrl?: string, apiKey: string, fetchImpl?: typeof fetch, timers?: object}} config
 */
export function anthropicAdapter({ baseUrl = "https://api.anthropic.com", apiKey = "", fetchImpl = globalThis.fetch?.bind(globalThis), timers } = {}) {
  const allowed = endpointAllowed(baseUrl);
  return Object.freeze({
    kind: "anthropic",
    endpoint: allowed ? endpointHost(baseUrl) : "",
    async complete(request, { signal, onChunk } = {}) {
      if (!allowed) return aiError("provider-unconfigured", "The endpoint must be https, or http to this computer.");
      if (!apiKey) return aiError("auth-failed", "An Anthropic key is required.");
      return httpCall({
        fetchImpl,
        url: joinUrl(baseUrl, "v1/messages"),
        headers: {
          "content-type": "application/json",
          "x-api-key": apiKey,
          "anthropic-version": ANTHROPIC_VERSION,
          "anthropic-dangerous-direct-browser-access": "true",
        },
        body: toAnthropicRequest(request),
        stream: !!request.stream,
        signal,
        onChunk,
        timers,
        toChunk: anthropicStreamMapper(),
        toResponse: fromAnthropicResponse,
        bodyToError: (json, status, extra) => fromAnthropicError(json?.error ?? json, status, extra),
      });
    },
  });
}

// ---- The host callback -------------------------------------------------------

/**
 * The host adapter. `callback(request, {signal, onChunk, purpose})` is the
 * host's `aiProvider`, in process, or the `postMessage` channel's `ask`; it
 * resolves to a Chat Completions response (or an OpenAI error object), and may
 * call `onChunk` with chunk objects while it streams. Its answer is checked
 * like any endpoint's.
 *
 * @param {(request: object, options: object) => Promise<object>} callback
 * @param {{label?: string}} [options]
 */
export function hostAdapter(callback, { label = "" } = {}) {
  return Object.freeze({
    kind: "host",
    endpoint: label,
    async complete(request, { signal, onChunk, purpose = "chat" } = {}) {
      if (signal?.aborted) return aiError("cancelled", "Stopped.");
      const assembler = createAssembler();
      let streamed = false;
      let answer;
      try {
        answer = await callback(request, {
          signal,
          purpose,
          onChunk(raw) {
            const checked = checkChunk(raw);
            if (!checked) return;
            streamed = true;
            assembler.push(checked);
            try {
              onChunk?.(checked);
            } catch (err) {
              console.warn("opendoc: an AI chunk listener threw", err);
            }
          },
        });
      } catch (err) {
        if (signal?.aborted || err?.name === "AbortError") return aiError("cancelled", "Stopped.", assembler.text() ? { partial: assembler.text() } : {});
        if (err?.message === "timeout") return aiError("timeout", "The host did not answer in time.");
        return aiError("host-refused", String(err?.message ?? err));
      }
      if (signal?.aborted) return aiError("cancelled", "Stopped.", assembler.text() ? { partial: assembler.text() } : {});
      if (answer?.error) return fromOpenAiError(answer.error, Number(answer.status) || 0);
      if (answer == null && streamed) return assembler.response();
      return checkResponse(answer);
    },
  });
}

// ---- MCP sampling ------------------------------------------------------------

/** MCP `stopReason` → `finish_reason`. */
const MCP_FINISH = Object.freeze({ endTurn: "stop", stopSequence: "stop", maxTokens: "length", toolUse: "tool_calls" });

/** A contract request as `sampling/createMessage` params. Text only. Pure. */
export function toSamplingParams(request) {
  const system = [];
  const messages = [];
  for (const message of request.messages) {
    if (message.role === "system") system.push(textOf(message.content));
    else if (message.role === "user" || message.role === "assistant") {
      const text = textOf(message.content);
      if (text) messages.push({ role: message.role, content: { type: "text", text } });
    }
  }
  const params = { messages, maxTokens: request.max_completion_tokens };
  if (system.length) params.systemPrompt = system.join("\n\n");
  if (request.temperature !== undefined) params.temperature = request.temperature;
  if (request.stop) params.stopSequences = request.stop;
  if (request.model) params.modelPreferences = { hints: [{ name: request.model }] };
  return params;
}

/** A `CreateMessageResult` as a contract response. Pure. */
export function fromSamplingResult(result) {
  const blocks = Array.isArray(result?.content) ? result.content : result?.content ? [result.content] : [];
  if (!blocks.length) return aiError("bad-response", "The MCP host returned no content.");
  const text = blocks.filter((b) => b?.type === "text").map((b) => b.text ?? "").join("");
  const calls = blocks
    .filter((b) => b?.type === "tool_use")
    .map((b) => ({ id: String(b.id), type: "function", function: { name: String(b.name), arguments: JSON.stringify(b.input ?? {}) } }));
  return checkResponse({
    id: "",
    model: result.model ?? "",
    choices: [{ index: 0, message: { role: "assistant", content: text || (calls.length ? null : ""), ...(calls.length ? { tool_calls: calls } : {}) }, finish_reason: MCP_FINISH[result.stopReason] ?? "stop" }],
  });
}

/**
 * The MCP sampling adapter: `requester(method, params, {signal})` sends one
 * `sampling/createMessage` through whatever MCP connection the host holds and
 * resolves to the client's result. No streaming; one chunk at the end, so a
 * streaming caller still renders the answer.
 *
 * @param {(method: string, params: object, options: {signal?: AbortSignal}) => Promise<object>} requester
 */
export function mcpSamplingAdapter(requester, { label = "" } = {}) {
  return Object.freeze({
    kind: "mcp-sampling",
    endpoint: label,
    async complete(request, { signal, onChunk } = {}) {
      if (signal?.aborted) return aiError("cancelled", "Stopped.");
      let result;
      try {
        result = await requester("sampling/createMessage", toSamplingParams(request), { signal });
      } catch (err) {
        if (signal?.aborted || err?.name === "AbortError") return aiError("cancelled", "Stopped.");
        return aiError("host-refused", String(err?.message ?? err));
      }
      const answer = fromSamplingResult(result);
      if (answer.ok && request.stream && answer.response.choices[0].message.content) {
        try {
          onChunk?.({ id: "", object: "chat.completion.chunk", model: answer.response.model, choices: [{ index: 0, delta: { content: answer.response.choices[0].message.content }, finish_reason: answer.response.choices[0].finish_reason }] });
        } catch (err) {
          console.warn("opendoc: an AI chunk listener threw", err);
        }
      }
      return answer;
    },
  });
}
