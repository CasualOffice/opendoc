// SPDX-License-Identifier: Apache-2.0
// The editor's end of a plugin's port: JSON-RPC 2.0 (<https://www.jsonrpc.org/specification>)
// over one `MessagePort` (`docs/183` §5).
//
// The port is the plugin's ONLY channel. It was transferred into the plugin's
// frame once, at connect, so no other frame holds the other end and nothing on
// `window` is ever read for a plugin: an opaque-origin frame has no origin to
// check, so the channel itself is the identity.
//
// Bounds, all answered as JSON-RPC errors with our code in `data.code`, never
// thrown: a message over `maxBytes`, more than `maxInFlight` requests at once,
// more than `perSecond` requests in a second. Everything is a value; a handler
// that throws becomes an `internal` error with its message.
//
// Complexity: O(message) per message; O(1) state per request in flight.

/** JSON-RPC's own error codes. */
export const RPC_ERRORS = Object.freeze({
  parse: -32700,
  invalidRequest: -32600,
  methodNotFound: -32601,
  invalidParams: -32602,
  internal: -32603,
  /** Ours: a refusal by policy (permission, rate, bounds), with `data.code`. */
  refused: -32000,
});

/** The bounds of one plugin port. */
export const RPC_BOUNDS = Object.freeze({ maxBytes: 4 * 1024 * 1024, maxInFlight: 64, perSecond: 20, streamPerSecond: 40 });

/** The live co-editor's methods (`docs/183` §23) count against their OWN window,
 *  `streamPerSecond`: a stream of batched writes must not starve the add-in's
 *  other calls, and Stop or a rollback must never be refused because the stream
 *  used the second up. The co-editor bounds what the writes carry. */
const STREAMED = (method) => method.startsWith("edit/");

/** A refusal value a handler may return to answer with a policy error. */
export class RpcRefusal extends Error {
  constructor(code, message, data = {}) {
    super(message);
    this.code = code;
    this.data = data;
  }
}

const sizeOf = (value) => {
  try {
    return new TextEncoder().encode(JSON.stringify(value)).length;
  } catch {
    return Infinity;
  }
};

/**
 * Serves `handlers` on `port`.
 *
 * @param {MessagePort} port
 * @param {Record<string, (params: object, context: {id: unknown, signal: AbortSignal}) => unknown>} handlers
 * @param {{now?: () => number, bounds?: object, onError?: (error: unknown) => void}} [options]
 * @returns {{notify: (method: string, params?: object) => void, close: () => void, inFlight: () => number}}
 */
export function servePort(port, handlers, { now = () => Date.now(), bounds = RPC_BOUNDS, onError = () => {} } = {}) {
  const inFlight = new Map();
  let windowStart = 0;
  let streamStart = 0;
  let streamCount = 0;
  let windowCount = 0;
  let closed = false;

  const send = (message) => {
    if (closed) return;
    try {
      port.postMessage({ jsonrpc: "2.0", ...message });
    } catch (err) {
      onError(err);
    }
  };
  const error = (id, code, message, data) => send({ id: id ?? null, error: { code, message, ...(data ? { data } : {}) } });

  async function handle(message) {
    if (!message || typeof message !== "object" || message.jsonrpc !== "2.0" || typeof message.method !== "string") {
      error(message?.id, RPC_ERRORS.invalidRequest, "Not a JSON-RPC 2.0 request.");
      return;
    }
    const { id, method } = message;
    const isNotification = id === undefined;
    if (sizeOf(message) > bounds.maxBytes) {
      if (!isNotification) error(id, RPC_ERRORS.refused, "The message is too large.", { code: "bad-request" });
      return;
    }
    const at = now();
    const streamed = STREAMED(method) && Number.isFinite(bounds.streamPerSecond);
    if (streamed) {
      if (at - streamStart >= 1000) {
        streamStart = at;
        streamCount = 0;
      }
      streamCount += 1;
    } else {
      if (at - windowStart >= 1000) {
        windowStart = at;
        windowCount = 0;
      }
      windowCount += 1;
    }
    if (streamed ? streamCount > bounds.streamPerSecond : windowCount > bounds.perSecond) {
      if (!isNotification) error(id, RPC_ERRORS.refused, "Too many requests.", { code: "rate-limited" });
      return;
    }
    const handler = Object.prototype.hasOwnProperty.call(handlers, method) ? handlers[method] : null;
    if (!handler) {
      if (!isNotification) error(id, RPC_ERRORS.methodNotFound, `No method ${method}.`, { code: "unknown-method" });
      return;
    }
    if (!isNotification && inFlight.size >= bounds.maxInFlight) {
      error(id, RPC_ERRORS.refused, "Too many requests in flight.", { code: "rate-limited" });
      return;
    }
    const controller = new AbortController();
    if (!isNotification) inFlight.set(id, controller);
    try {
      const params = message.params && typeof message.params === "object" ? message.params : {};
      // `cancel(otherId)` aborts another request of this plugin's (`ai/cancel`).
      const cancel = (other) => {
        inFlight.get(other)?.abort();
        return inFlight.has(other);
      };
      const result = await handler(params, { id, signal: controller.signal, cancel });
      if (!isNotification) send({ id, result: result === undefined ? null : result });
    } catch (err) {
      if (isNotification) return;
      if (err instanceof RpcRefusal) error(id, RPC_ERRORS.refused, err.message, { code: err.code, ...err.data });
      else error(id, RPC_ERRORS.internal, String(err?.message ?? err), { code: "internal" });
    } finally {
      inFlight.delete(id);
    }
  }

  const onMessage = (event) => {
    if (closed) return;
    void handle(event.data);
  };
  port.addEventListener("message", onMessage);
  port.start?.();

  return {
    /** A notification to the plugin (no reply expected). */
    notify(method, params = {}) {
      send({ method, params });
    },
    /** Aborts what is in flight and closes the port. Idempotent. */
    close() {
      if (closed) return;
      for (const controller of inFlight.values()) controller.abort();
      inFlight.clear();
      closed = true;
      port.removeEventListener("message", onMessage);
      try {
        port.close();
      } catch {
        /* already closed */
      }
    },
    inFlight: () => inFlight.size,
  };
}
