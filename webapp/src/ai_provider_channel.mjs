// SPDX-License-Identifier: Apache-2.0
// Which model answers, and how a request reaches it (`docs/183` §3.8).
//
// RESOLUTION, strongest first, asked on every call:
//
//   1. the host, in process: `window.opendoc.aiProvider = fn` (or the element's
//      `aiProvider` property);
//   2. the host, over `postMessage` (`?aiProvider=host`): the `ai-provider`
//      event out, the `ai-provider.answer` request back, correlated by `qid` —
//      the directory seam's shape (`docs/175` §7.2, `directory.mjs`);
//   3. MCP sampling, when a requester was supplied;
//   4. bring your own key, standalone only (`ai_settings.mjs` stores it);
//   5. none — and then NOTHING is sent: no AI command runs and no network call
//      is made (owner, 2026-10-30).
//
// `?aiProvider=off` withdraws all five. A framed editor never uses BYOK: a key
// typed into someone else's product would sit on an origin that product does
// not control, and the host has its own channel.
//
// AUDIT (`docs/183` §12.4, `docs/178` §5.9's rule): each call is recorded with
// its purpose, the provider kind and endpoint, the model, token usage, the
// duration and the result code — NEVER the prompt, the document text or the
// answer. The record is kept for this page and offered to `onAudit`.
//
// Complexity: O(1) per call beyond the adapter's own work.
import { LIMITS, aiError, checkRequest, createBudget } from "./ai_provider.mjs";
import { anthropicAdapter, hostAdapter, mcpSamplingAdapter, openAiAdapter } from "./ai_provider_adapters.mjs";

/** The URL parameter a framed editor reads (`hostConfig().aiProvider`). */
export const AI_PROVIDER_PARAM = "aiProvider";
/** Its two values. */
export const AI_PROVIDER_HOST = "host";
export const AI_PROVIDER_OFF = "off";
/** How long a `postMessage` host has to answer one request. */
export const HOST_TIMEOUT_MS = 120_000;
/** What one request may be for: the host's audit and routing, never the model's. */
export const AI_PURPOSES = Object.freeze(["chat", "rewrite", "draft", "summarise", "translate", "review", "comment", "test", "embed", "plugin"]);
/** How many audit records a page keeps. */
export const AUDIT_LIMIT = 200;

/**
 * The editor's side of the `postMessage` host channel.
 *
 * @param {{emit: (name: string, detail: object) => void, setTimer?: Function, clearTimer?: Function, timeoutMs?: number}} io
 */
export function createHostProviderChannel({ emit, setTimer: rawSetTimer = setTimeout, clearTimer = clearTimeout, timeoutMs = HOST_TIMEOUT_MS }) {
  // A waiting request must not keep a headless runtime alive on its own (Node's
  // `unref`); browsers have no such method and lose nothing.
  const setTimer = (fn, ms) => {
    const handle = rawSetTimer(fn, ms);
    handle?.unref?.();
    return handle;
  };
  let counter = 0;
  /** qid -> {resolve, reject, onChunk, timer}. Bounded by the callers' budgets. */
  const waiting = new Map();

  function finish(qid) {
    const waiter = waiting.get(qid);
    if (!waiter) return null;
    waiting.delete(qid);
    clearTimer(waiter.timer);
    return waiter;
  }

  return {
    /** The adapter callback: one request out, the answer back. */
    ask(request, { signal, onChunk, purpose = "chat" } = {}) {
      const qid = `ai-provider:${(counter += 1)}`;
      return new Promise((resolve, reject) => {
        const timer = setTimer(() => {
          if (!finish(qid)) return;
          emit("ai-provider", { qid, kind: "cancel", request: null, purpose });
          reject(new Error("timeout"));
        }, timeoutMs);
        waiting.set(qid, { resolve, reject, onChunk, timer });
        signal?.addEventListener?.(
          "abort",
          () => {
            if (!finish(qid)) return;
            emit("ai-provider", { qid, kind: "cancel", request: null, purpose });
            reject(Object.assign(new Error("aborted"), { name: "AbortError" }));
          },
          { once: true },
        );
        emit("ai-provider", { qid, kind: "request", request, purpose });
      });
    },
    /** `ai-provider.answer`: `{qid, chunk}` while streaming, then `{qid,
     *  response}` or `{qid, error, status?}`. Told apart by which field is
     *  present, because `kind` and `type` belong to the envelope. Returns
     *  whether the `qid` was waiting; an unknown or late `qid` is ignored. */
    answer(message) {
      const qid = String(message?.qid ?? "");
      const waiter = waiting.get(qid);
      if (!waiter) return false;
      if (message.chunk !== undefined && message.response === undefined && message.error === undefined) {
        clearTimer(waiter.timer);
        waiter.timer = setTimer(() => {
          if (!finish(qid)) return;
          waiter.reject(new Error("timeout"));
        }, timeoutMs);
        waiter.onChunk?.(message.chunk);
        return true;
      }
      finish(qid);
      if (message.error !== undefined) waiter.resolve({ error: message.error ?? { message: "The host refused." }, status: message.status ?? 0 });
      else waiter.resolve(message.response ?? null);
      return true;
    },
    inFlight: () => waiting.size,
  };
}

/** The adapter a BYOK configuration builds, or null. */
export function adapterForSettings(settings, { fetchImpl, timers } = {}) {
  if (!settings || !settings.baseUrl) return null;
  if (settings.protocol === "anthropic") {
    return anthropicAdapter({ baseUrl: settings.baseUrl, apiKey: settings.apiKey ?? "", fetchImpl, timers });
  }
  return openAiAdapter({
    baseUrl: settings.baseUrl,
    apiKey: settings.apiKey ?? "",
    azure: settings.protocol === "azure",
    apiVersion: settings.apiVersion,
    legacyMaxTokens: settings.legacyMaxTokens === true,
    fetchImpl,
    timers,
  });
}

/**
 * The provider service the editor and plugins call. One per page.
 *
 * @param {object} io
 * @param {{aiProvider?: string|null}} io.config `hostConfig()`
 * @param {boolean} io.framed whether the editor is inside someone else's page
 * @param {() => object|null} io.settings the stored BYOK settings, read per call
 * @param {object|null} [io.remote] `createHostProviderChannel`'s value when `?aiProvider=host`
 * @param {() => Function|null} [io.hostFn] the host session's in-process
 *   `aiProvider`, read per call so a host may set it at any time
 * @param {object} [io.budget] `{requests, tokens}`
 * @param {(record: object) => void} [io.onAudit]
 * @param {typeof fetch} [io.fetchImpl]
 * @param {() => number} [io.now]
 */
export function createAiProviders({ config = {}, framed = false, settings = () => null, remote = null, hostFn = () => null, budget = {}, onAudit = () => {}, fetchImpl, timers, now = () => Date.now() }) {
  const mode = String(config?.aiProvider ?? "").toLowerCase();
  const off = mode === AI_PROVIDER_OFF;
  let local = null;
  let sampling = null;
  const spend = createBudget(budget);
  const audit = [];

  /** The adapter in force now, and the configured limits, or null. */
  function current() {
    if (off) return null;
    const fromSession = typeof hostFn === "function" ? hostFn() : null;
    const fn = local?.fn ?? (typeof fromSession === "function" ? fromSession : null);
    if (fn) return { adapter: hostAdapter(fn, { label: local?.label ?? "" }), model: "", contextTokens: LIMITS.defaultContextTokens, outputCeiling: LIMITS.defaultOutputTokens };
    if (remote && mode === AI_PROVIDER_HOST) return { adapter: hostAdapter((request, options) => remote.ask(request, options)), model: "", contextTokens: LIMITS.defaultContextTokens, outputCeiling: LIMITS.defaultOutputTokens };
    if (sampling) return { adapter: mcpSamplingAdapter(sampling.requester, { label: sampling.label }), model: "", contextTokens: LIMITS.defaultContextTokens, outputCeiling: LIMITS.defaultOutputTokens };
    if (framed) return null;
    const stored = settings();
    const adapter = adapterForSettings(stored, { fetchImpl, timers });
    if (!adapter || !adapter.endpoint) return null;
    return {
      adapter,
      model: String(stored.model ?? ""),
      contextTokens: Number(stored.contextTokens) || LIMITS.defaultContextTokens,
      outputCeiling: Number(stored.outputCeiling) || LIMITS.defaultOutputTokens,
    };
  }

  function record(entry) {
    const frozen = Object.freeze(entry);
    audit.push(frozen);
    if (audit.length > AUDIT_LIMIT) audit.shift();
    try {
      onAudit(frozen);
    } catch (err) {
      console.warn("opendoc: an AI audit listener threw", err);
    }
  }

  return {
    /** Whether a request would go anywhere at all. O(1). */
    available: () => current() !== null,
    /** Why none is available, as a code: `off` (the host withdrew AI), `framed`
     *  (embedded, and the host offers no provider), `unconfigured`. O(1). */
    unavailableReason() {
      if (current()) return null;
      if (off) return "off";
      return framed ? "framed" : "unconfigured";
    },
    /** What the disclosure names (`docs/183` §3.9): the provider kind and the
     *  endpoint that will receive text, before anything is sent. O(1). */
    describe() {
      const now_ = current();
      if (!now_) return Object.freeze({ available: false, kind: null, endpoint: "", reason: this.unavailableReason() });
      return Object.freeze({ available: true, kind: now_.adapter.kind, endpoint: now_.adapter.endpoint, model: now_.model, reason: null });
    },
    /** The in-process host provider (`window.opendoc.aiProvider`). */
    get hostProvider() {
      return local?.fn ?? null;
    },
    set hostProvider(fn) {
      local = typeof fn === "function" ? { fn, label: "" } : null;
    },
    /** An MCP host lending its model: `requester(method, params, {signal})`. */
    useSampling(requester, label = "") {
      sampling = typeof requester === "function" ? { requester, label } : null;
    },
    /**
     * One request through the contract: checked, budgeted, sent, checked again,
     * audited. Resolves `{ok: true, response, estimate}` or `{ok: false, error}`;
     * never rejects.
     *
     * @param {object} request a Chat Completions request body
     * @param {{signal?: AbortSignal, onChunk?: Function, purpose?: string}} [options]
     */
    async complete(request, { signal, onChunk, purpose = "chat" } = {}) {
      const started = now();
      const target = current();
      const why = String(purpose);
      const purposeOk = AI_PURPOSES.includes(why) ? why : "plugin";
      if (!target) {
        const reason = this.unavailableReason();
        return aiError(reason === "off" ? "provider-disabled" : "provider-unconfigured", "No AI provider is configured.", { reason });
      }
      const checked = checkRequest(request, { model: target.model, contextTokens: target.contextTokens, outputCeiling: target.outputCeiling });
      const base = { purpose: purposeOk, kind: target.adapter.kind, endpoint: target.adapter.endpoint, model: checked.ok ? checked.request.model : String(request?.model ?? "") };
      if (!checked.ok) {
        record({ ...base, at: started, ms: 0, code: checked.error.code, usage: null });
        return checked;
      }
      const charged = spend.charge(checked.estimate, checked.request.max_completion_tokens);
      if (!charged.ok) {
        record({ ...base, at: started, ms: 0, code: charged.error.code, usage: null });
        return charged;
      }
      let answer;
      try {
        answer = await target.adapter.complete(checked.request, { signal, onChunk, purpose: purposeOk });
      } catch (err) {
        // Adapters resolve; a throw is a bug in one, reported as a value anyway.
        answer = aiError("bad-response", String(err?.message ?? err));
      }
      const usage = answer.ok ? (answer.response.usage ?? null) : null;
      spend.settle(charged.projected, usage);
      record({ ...base, at: started, ms: now() - started, code: answer.ok ? "ok" : answer.error.code, usage, estimate: checked.estimate });
      return answer.ok ? Object.freeze({ ...answer, estimate: checked.estimate }) : answer;
    },
    /** This page's audit records, oldest first. Never text. */
    audit: () => Object.freeze([...audit]),
    /** What is left of this page's budget. */
    remaining: () => spend.remaining(),
  };
}
