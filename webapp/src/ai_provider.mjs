// SPDX-License-Identifier: Apache-2.0
// THE OPENDOC AI PROVIDER CONTRACT, version 1 (`docs/183` §3, ADR-077).
//
// NOT A NEW PROTOCOL. A request is an OpenAI Chat Completions request body, a
// response is a Chat Completions response, a stream is a sequence of
// `chat.completion.chunk` objects, an error is the OpenAI error object
// (<https://platform.openai.com/docs/api-reference/chat/create>). That one wire
// format is what Azure OpenAI, Mistral, OpenRouter, Gemini's compatibility
// endpoint, Ollama, vLLM and LM Studio all accept, so one URL and one key reach
// them all; `ai_provider_adapters.mjs` maps Anthropic Messages and MCP sampling
// onto it. Our only additions travel BESIDE the body — `purpose` and the abort
// signal — never inside it, so a host proxy forwards the body unchanged.
//
// This module is the contract and nothing else: what may be sent
// (`checkRequest`), what may come back (`checkResponse`, `checkChunk`), how a
// stream is assembled, how errors are normalised, the token estimate and the
// budgets. Pure: no DOM, no fetch, no `t()`. The JSON Schemas a host validates
// against are `webapp/schemas/ai-provider-*.schema.json`; `ai_provider.test.mjs`
// holds this module and those files to the same field lists.
//
// Complexity: every check is O(request size); nothing here touches the document.

/** The contract's version. Bumped only when a field changes meaning or goes. */
export const AI_CONTRACT_VERSION = 1;

/** The roles a message may carry (Chat Completions). */
export const MESSAGE_ROLES = Object.freeze(["system", "user", "assistant", "tool"]);

/** Every top-level request field the editor may send, and nothing else. A field
 *  outside this list is refused before anything leaves (`docs/183` §3.2), so a
 *  plugin cannot smuggle a provider-specific field past the contract. */
export const REQUEST_FIELDS = Object.freeze([
  "model",
  "messages",
  "max_completion_tokens",
  "temperature",
  "stop",
  "stream",
  "stream_options",
  "tools",
  "tool_choice",
  "response_format",
]);

/** `finish_reason` values a response may carry. */
export const FINISH_REASONS = Object.freeze(["stop", "length", "content_filter", "tool_calls"]);

/** The normalised error codes (`docs/183` §3.3). What the UI and a plugin branch on. */
export const AI_ERROR_CODES = Object.freeze([
  "provider-unconfigured",
  "provider-disabled",
  "bad-request",
  "context-too-long",
  "over-budget",
  "auth-failed",
  "rate-limited",
  "content-filtered",
  "server-error",
  "network",
  "timeout",
  "cancelled",
  "bad-response",
  "host-refused",
]);

/** The contract's bounds. Defaults a host may lower and the settings may change
 *  within; `LIMITS.maxOutputTokens` is the hard ceiling on any one answer. */
export const LIMITS = Object.freeze({
  maxMessages: 256,
  maxStop: 4,
  maxOutputTokens: 32768,
  defaultOutputTokens: 4096,
  defaultContextTokens: 32000,
  defaultSessionRequests: 200,
  defaultSessionTokens: 2_000_000,
  maxTools: 64,
  /** No first byte within this long is `timeout`. */
  firstByteMs: 30_000,
  /** No chunk within this long, once streaming, is `timeout`. */
  idleMs: 60_000,
  /** A request body larger than this is refused (4 MiB, the plugin port's bound too). */
  maxRequestBytes: 4 * 1024 * 1024,
});

const isObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const utf8 = (text) => new TextEncoder().encode(text).length;

/** A frozen error value: `{ok: false, error: {code, message, ...}}`. O(1). */
export function aiError(code, message = "", extra = {}) {
  const known = AI_ERROR_CODES.includes(code) ? code : "bad-response";
  return Object.freeze({ ok: false, error: Object.freeze({ code: known, message: String(message), ...extra }) });
}

/** One message's text, for the estimate. Content parts other than text count by
 *  their JSON, which over-estimates — the bound errs towards sending less. */
function messageText(message) {
  const { content } = message;
  if (typeof content === "string") return content;
  if (Array.isArray(content)) return content.map((part) => (part?.type === "text" ? String(part.text ?? "") : JSON.stringify(part))).join("");
  return content == null ? "" : JSON.stringify(content);
}

/**
 * The declared token estimate (`docs/178` §4.5): UTF-8 bytes ÷ 4, rounded up,
 * plus a floor of 4 per message for the role framing. Named `bytes/4` wherever
 * it is reported, because a real count needs the model's own vocabulary.
 *
 * O(request size).
 */
export function estimateTokens(request) {
  let total = 0;
  for (const message of request?.messages ?? []) {
    total += 4 + Math.ceil(utf8(messageText(message)) / 4);
    if (message?.tool_calls) total += Math.ceil(utf8(JSON.stringify(message.tool_calls)) / 4);
  }
  if (request?.tools) total += Math.ceil(utf8(JSON.stringify(request.tools)) / 4);
  return total;
}

function checkContent(content, where) {
  if (typeof content === "string") return null;
  if (content === null) return null; // an assistant message carrying only tool calls
  if (!Array.isArray(content)) return `${where}.content must be a string or an array of text parts`;
  for (const [index, part] of content.entries()) {
    if (!isObject(part) || part.type !== "text" || typeof part.text !== "string") {
      return `${where}.content[${index}] must be {type: "text", text}`;
    }
  }
  return null;
}

function checkMessage(message, index) {
  const where = `messages[${index}]`;
  if (!isObject(message)) return `${where} must be an object`;
  if (!MESSAGE_ROLES.includes(message.role)) return `${where}.role must be one of ${MESSAGE_ROLES.join(", ")}`;
  for (const key of Object.keys(message)) {
    if (!["role", "content", "name", "tool_call_id", "tool_calls"].includes(key)) return `${where}.${key} is not in the contract`;
  }
  const contentProblem = checkContent(message.content ?? null, where);
  if (contentProblem) return contentProblem;
  if (message.role === "tool" && typeof message.tool_call_id !== "string") return `${where}.tool_call_id is required for a tool message`;
  if (message.tool_calls !== undefined) {
    if (message.role !== "assistant" || !Array.isArray(message.tool_calls)) return `${where}.tool_calls belongs to an assistant message`;
    for (const call of message.tool_calls) {
      if (!isObject(call) || call.type !== "function" || typeof call.id !== "string" || !isObject(call.function) || typeof call.function.name !== "string") {
        return `${where}.tool_calls entries must be {id, type: "function", function: {name, arguments}}`;
      }
    }
  }
  return null;
}

function checkTools(tools) {
  if (!Array.isArray(tools) || tools.length > LIMITS.maxTools) return `tools must be an array of at most ${LIMITS.maxTools}`;
  for (const [index, tool] of tools.entries()) {
    if (!isObject(tool) || tool.type !== "function" || !isObject(tool.function) || !/^[a-zA-Z0-9_-]{1,64}$/.test(tool.function.name ?? "")) {
      return `tools[${index}] must be {type: "function", function: {name, description?, parameters?}}`;
    }
  }
  return null;
}

/**
 * Checks and normalises a request against the contract. Returns
 * `{ok: true, request, estimate}` with `max_completion_tokens` always set (the
 * caller's value clamped to `outputCeiling`), or an `aiError`.
 *
 * Refused locally, so NOTHING is sent: an unknown field, a malformed message,
 * a request over `maxRequestBytes`, and an input estimate over `contextTokens`
 * (`context-too-long`).
 *
 * O(request size).
 *
 * @param {object} raw
 * @param {{model?: string, contextTokens?: number, outputCeiling?: number}} [limits]
 */
export function checkRequest(raw, { model = "", contextTokens = LIMITS.defaultContextTokens, outputCeiling = LIMITS.defaultOutputTokens } = {}) {
  if (!isObject(raw)) return aiError("bad-request", "The request must be a JSON object.");
  for (const key of Object.keys(raw)) {
    if (!REQUEST_FIELDS.includes(key)) return aiError("bad-request", `"${key}" is not in the OpenDoc AI provider contract.`, { field: key });
  }
  const messages = raw.messages;
  if (!Array.isArray(messages) || messages.length === 0 || messages.length > LIMITS.maxMessages) {
    return aiError("bad-request", `messages must hold 1 to ${LIMITS.maxMessages} messages.`, { field: "messages" });
  }
  for (const [index, message] of messages.entries()) {
    const problem = checkMessage(message, index);
    if (problem) return aiError("bad-request", problem, { field: "messages" });
  }
  if (raw.model !== undefined && typeof raw.model !== "string") return aiError("bad-request", "model must be a string.", { field: "model" });
  if (raw.temperature !== undefined && !(typeof raw.temperature === "number" && raw.temperature >= 0 && raw.temperature <= 2)) {
    return aiError("bad-request", "temperature must be between 0 and 2.", { field: "temperature" });
  }
  if (raw.stop !== undefined && !(Array.isArray(raw.stop) && raw.stop.length <= LIMITS.maxStop && raw.stop.every((s) => typeof s === "string"))) {
    return aiError("bad-request", `stop must be at most ${LIMITS.maxStop} strings.`, { field: "stop" });
  }
  if (raw.stream !== undefined && typeof raw.stream !== "boolean") return aiError("bad-request", "stream must be a boolean.", { field: "stream" });
  if (raw.stream_options !== undefined && !(raw.stream === true && isObject(raw.stream_options) && Object.keys(raw.stream_options).every((k) => k === "include_usage"))) {
    return aiError("bad-request", "stream_options is {include_usage} and needs stream: true.", { field: "stream_options" });
  }
  if (raw.tools !== undefined) {
    const problem = checkTools(raw.tools);
    if (problem) return aiError("bad-request", problem, { field: "tools" });
  }
  if (raw.tool_choice !== undefined && !(["auto", "none", "required"].includes(raw.tool_choice) || (isObject(raw.tool_choice) && raw.tool_choice.type === "function"))) {
    return aiError("bad-request", "tool_choice must be auto, none, required or a function choice.", { field: "tool_choice" });
  }
  if (raw.response_format !== undefined && !(isObject(raw.response_format) && ["text", "json_object", "json_schema"].includes(raw.response_format.type))) {
    return aiError("bad-request", "response_format.type must be text, json_object or json_schema.", { field: "response_format" });
  }
  const asked = raw.max_completion_tokens;
  if (asked !== undefined && !(Number.isInteger(asked) && asked >= 1 && asked <= LIMITS.maxOutputTokens)) {
    return aiError("bad-request", `max_completion_tokens must be an integer from 1 to ${LIMITS.maxOutputTokens}.`, { field: "max_completion_tokens" });
  }
  const ceiling = Math.max(1, Math.min(LIMITS.maxOutputTokens, Math.floor(outputCeiling)));
  const request = { ...raw, max_completion_tokens: Math.min(asked ?? ceiling, ceiling) };
  if (!request.model) request.model = model;
  let bytes;
  try {
    bytes = utf8(JSON.stringify(request));
  } catch {
    return aiError("bad-request", "The request is not serialisable as JSON.");
  }
  if (bytes > LIMITS.maxRequestBytes) return aiError("bad-request", "The request is larger than the contract allows.", { bytes });
  const estimate = estimateTokens(request);
  if (estimate > contextTokens) {
    return aiError("context-too-long", "The text is too long for the configured context window.", { estimate, limit: contextTokens, estimator: "bytes/4" });
  }
  return Object.freeze({ ok: true, request: Object.freeze(request), estimate });
}

function usageOf(raw) {
  if (!isObject(raw)) return null;
  const prompt = Number(raw.prompt_tokens);
  const completion = Number(raw.completion_tokens);
  if (!Number.isFinite(prompt) || !Number.isFinite(completion)) return null;
  return Object.freeze({ prompt_tokens: prompt, completion_tokens: completion, total_tokens: Number.isFinite(Number(raw.total_tokens)) ? Number(raw.total_tokens) : prompt + completion });
}

function checkToolCalls(calls) {
  if (calls === undefined) return true;
  return Array.isArray(calls) && calls.every((call) => isObject(call) && typeof call.id === "string" && isObject(call.function) && typeof call.function.name === "string" && typeof (call.function.arguments ?? "") === "string");
}

/**
 * Checks a Chat Completions response and returns `{ok: true, response}` with
 * exactly the contract's fields (anything else the endpoint added is dropped),
 * or `bad-response`. A host's answer is checked like any endpoint's: a host is
 * trusted with its own model, not with the shape of our state.
 *
 * O(response size).
 */
export function checkResponse(raw) {
  if (isObject(raw?.error)) return fromOpenAiError(raw.error, 0);
  if (!isObject(raw) || !Array.isArray(raw.choices) || raw.choices.length === 0) {
    return aiError("bad-response", "The answer was not a Chat Completions response.");
  }
  const choices = [];
  for (const [index, choice] of raw.choices.entries()) {
    const message = choice?.message;
    if (!isObject(message) || (message.content != null && typeof message.content !== "string") || !checkToolCalls(message.tool_calls)) {
      return aiError("bad-response", `choices[${index}].message is not a Chat Completions message.`);
    }
    const finish = FINISH_REASONS.includes(choice.finish_reason) ? choice.finish_reason : "stop";
    choices.push(Object.freeze({
      index: Number.isInteger(choice.index) ? choice.index : index,
      message: Object.freeze({ role: "assistant", content: message.content ?? null, ...(message.tool_calls ? { tool_calls: message.tool_calls } : {}) }),
      finish_reason: finish,
    }));
  }
  if (choices[0].finish_reason === "content_filter" && !choices[0].message.content) {
    return aiError("content-filtered", "The provider declined to answer.");
  }
  const usage = usageOf(raw.usage);
  return Object.freeze({
    ok: true,
    response: Object.freeze({
      id: String(raw.id ?? ""),
      object: "chat.completion",
      created: Number.isFinite(Number(raw.created)) ? Number(raw.created) : Math.floor(Date.now() / 1000),
      model: String(raw.model ?? ""),
      choices: Object.freeze(choices),
      ...(usage ? { usage } : {}),
    }),
  });
}

/** Checks one `chat.completion.chunk`. Returns the chunk with the contract's
 *  fields, or null for a malformed one (dropped and counted by the caller). */
export function checkChunk(raw) {
  if (!isObject(raw) || !Array.isArray(raw.choices)) return null;
  const choices = [];
  for (const choice of raw.choices) {
    const delta = isObject(choice?.delta) ? choice.delta : {};
    if (delta.content != null && typeof delta.content !== "string") return null;
    const clean = {};
    if (typeof delta.role === "string") clean.role = delta.role;
    if (typeof delta.content === "string") clean.content = delta.content;
    if (Array.isArray(delta.tool_calls)) clean.tool_calls = delta.tool_calls;
    choices.push({
      index: Number.isInteger(choice.index) ? choice.index : 0,
      delta: clean,
      finish_reason: FINISH_REASONS.includes(choice?.finish_reason) ? choice.finish_reason : null,
    });
  }
  const usage = usageOf(raw.usage);
  return Object.freeze({ id: String(raw.id ?? ""), object: "chat.completion.chunk", model: String(raw.model ?? ""), choices, ...(usage ? { usage } : {}) });
}

/**
 * Assembles checked chunks into one response, the way OpenAI's own SDKs do:
 * content concatenated, tool-call fragments merged by `index`, the last
 * `finish_reason` and `usage` kept. Used for every stream, so a caller that
 * ignores chunks gets the same answer as one that renders them.
 *
 * O(chunks).
 */
export function createAssembler() {
  let id = "";
  let model = "";
  let content = "";
  let finish = null;
  let usage = null;
  /** index -> {id, type, function: {name, arguments}} */
  const calls = new Map();
  return {
    push(chunk) {
      id ||= chunk.id;
      model ||= chunk.model;
      if (chunk.usage) usage = chunk.usage;
      for (const choice of chunk.choices) {
        if (choice.index !== 0) continue;
        if (choice.delta.content) content += choice.delta.content;
        for (const part of choice.delta.tool_calls ?? []) {
          const at = Number.isInteger(part?.index) ? part.index : calls.size;
          const call = calls.get(at) ?? { id: "", type: "function", function: { name: "", arguments: "" } };
          if (part.id) call.id = part.id;
          if (part.function?.name) call.function.name += part.function.name;
          if (part.function?.arguments) call.function.arguments += part.function.arguments;
          calls.set(at, call);
        }
        if (choice.finish_reason) finish = choice.finish_reason;
      }
    },
    /** The text so far, for a cancelled call's `partial`. */
    text: () => content,
    response() {
      const tool_calls = [...calls.entries()].sort(([a], [b]) => a - b).map(([, call]) => call);
      return checkResponse({
        id,
        model,
        created: Math.floor(Date.now() / 1000),
        choices: [{ index: 0, message: { role: "assistant", content: content || (tool_calls.length ? null : ""), ...(tool_calls.length ? { tool_calls } : {}) }, finish_reason: finish ?? "stop" }],
        ...(usage ? { usage } : {}),
      });
    },
  };
}

/** The code an HTTP status becomes. O(1). */
export function codeForStatus(status) {
  if (status === 401 || status === 403) return "auth-failed";
  if (status === 429) return "rate-limited";
  if (status === 408) return "timeout";
  if (status === 413) return "context-too-long";
  if (status >= 500) return "server-error";
  if (status >= 400) return "bad-request";
  return "bad-response";
}

/** An OpenAI error object (`{message, type, code, param}`) as an `aiError`.
 *  `context_length_exceeded` and `insufficient_quota` are the two codes that
 *  mean something other than their status. O(1). */
export function fromOpenAiError(error, status, extra = {}) {
  const providerCode = String(error?.code ?? error?.type ?? "");
  let code = codeForStatus(status);
  if (providerCode === "context_length_exceeded") code = "context-too-long";
  else if (providerCode === "insufficient_quota") code = "over-budget";
  else if (providerCode === "content_filter" || providerCode === "content_policy_violation") code = "content-filtered";
  else if (!status && providerCode) code = "host-refused";
  return aiError(code, String(error?.message ?? ""), { providerCode: providerCode || null, status: status || null, ...extra });
}

/**
 * A session budget (`docs/183` §3.6): a ceiling on requests and on estimated
 * tokens for this page. `charge` is asked BEFORE a request is sent, so a spent
 * budget sends nothing; `settle` replaces the estimate with the endpoint's own
 * usage when it reported one. O(1).
 */
export function createBudget({ requests = LIMITS.defaultSessionRequests, tokens = LIMITS.defaultSessionTokens } = {}) {
  let usedRequests = 0;
  let usedTokens = 0;
  return {
    charge(estimate, outputCeiling) {
      if (usedRequests + 1 > requests) return aiError("over-budget", "This page's AI request budget is spent.", { limit: requests, kind: "requests" });
      const projected = estimate + outputCeiling;
      if (usedTokens + projected > tokens) return aiError("over-budget", "This page's AI token budget is spent.", { limit: tokens, kind: "tokens" });
      usedRequests += 1;
      usedTokens += projected;
      return Object.freeze({ ok: true, projected });
    },
    settle(projected, usage) {
      if (!usage) return;
      usedTokens += Number(usage.total_tokens ?? 0) - projected;
      if (usedTokens < 0) usedTokens = 0;
    },
    remaining: () => Object.freeze({ requests: requests - usedRequests, tokens: Math.max(0, tokens - usedTokens) }),
  };
}

/** Whether `url` may receive document text: `https:`, or `http:` to loopback
 *  only, for a local model (`docs/183` §3.9). Credentials in the URL are refused.
 *  O(url). */
export function endpointAllowed(url) {
  let parsed;
  try {
    parsed = new URL(String(url ?? ""));
  } catch {
    return false;
  }
  if (parsed.username || parsed.password) return false;
  if (parsed.protocol === "https:") return true;
  if (parsed.protocol !== "http:") return false;
  return parsed.hostname === "localhost" || parsed.hostname === "127.0.0.1" || parsed.hostname === "[::1]";
}

/** The origin a disclosure names, `api.openai.com` or `localhost:11434`. O(url). */
export function endpointHost(url) {
  try {
    return new URL(String(url)).host;
  } catch {
    return "";
  }
}
