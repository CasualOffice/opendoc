// The `postMessage` transport, EDITOR side.
//
// `docs/126` phase 2's "one schema, two transports". This file is an envelope and
// an origin check over `host_session.mjs`; it decides nothing about commands,
// events or refusals. Every request type it serves is read from
// `PROTOCOL.requests` and forwarded to the session method of the SAME NAME, so a
// verb cannot exist on one transport and be missing from the other — the failure
// mode this phase exists to make impossible.
//
// THE ORIGIN CONTRACT. `parseOriginAllowlist` states what is accepted and why;
// this file is where the decision is enforced, in both directions:
//
//   * INBOUND — a message whose `event.origin` is not on the allowlist is
//     dropped, silently and without a reply. Not answered with an error: an
//     error reply is itself a signal to whoever is probing, and an editor that
//     tells an unknown origin "wrong origin" has told it that it is an OpenDoc
//     editor.
//   * OUTBOUND — every post names an explicit target origin. A result goes back
//     to the origin that asked for it; an event goes to each allowed origin in
//     turn, so the browser delivers it to the one that matches and drops the
//     rest. `postMessage(msg, "*")` appears NOWHERE in this file, and a guard
//     asserts that.
//
// That last part is the difference from ONLYOFFICE, whose transport posts
// everything to a wildcard — `wnd.postMessage(msg, "*")` at
// `reference/web-apps/apps/api/documents/api.js:1303-1306`, under a literal
// `// TODO: specify explicit origin`. Document content over a wildcard target in
// an editor that can be embedded anywhere is a security decision, not a default.
//
// No English: this module carries no user-facing string at all. A refusal's
// sentence is produced by the session, which was handed it by the chrome.
import {
  HOST_EVENT_NAMES,
  PROTOCOL,
  isRequestEnvelope,
  originAllowed,
  parseOriginAllowlist,
} from "./host_contract.mjs";

/** How many distinct peers the bridge will remember for event delivery.
 *
 *  Bounded because the map is keyed by something a page can produce at will: a
 *  host could frame the editor and then hand its window to any number of
 *  same-origin documents. Eight is far more than any real embed needs, and the
 *  parent is always addressed regardless of the map, so a full map never costs a
 *  host their own events. */
const MAX_PEERS = 8;

/** The query parameter a deployment uses to name additional host origins. */
export const HOST_ORIGIN_PARAM = "hostOrigin";

/**
 * Reads the origin allowlist for this editor page.
 *
 * Separated from `attachHostBridge` so the policy is answerable in node against
 * a plain object, with no `window` and no bridge.
 *
 * @param {{location?: {search?: string, origin?: string}}} view
 * @returns {readonly string[]}
 */
export function allowlistFor(view) {
  let asked = null;
  try {
    asked = new URLSearchParams(view?.location?.search ?? "").get(HOST_ORIGIN_PARAM);
  } catch {
    asked = null;
  }
  return parseOriginAllowlist(asked, view?.location?.origin ?? "");
}

/**
 * Attaches the bridge to a window.
 *
 * @param {object} io
 * @param {object} io.session the in-process session — the only thing this calls.
 * @param {Window} io.view the editor's own window.
 * @returns {{detach: () => void, allowlist: readonly string[], peers: () => number}}
 */
export function attachHostBridge({ session, view }) {
  const allowlist = allowlistFor(view);
  /** origin -> the window that spoke from it. Bounded by `MAX_PEERS`. */
  const peers = new Map();

  /** The request verbs, taken from the schema and bound to the session's methods
   *  of the same name. A verb in `PROTOCOL.requests` with no session method is a
   *  `TypeError` on first use and a failed guard before that, which is the point:
   *  the two surfaces are the same list or the build says so. */
  const verbs = new Map(
    PROTOCOL.requests.map((type) => [type, (message) => callVerb(type, message)]),
  );

  function callVerb(type, message) {
    if (type === "execute") return session.execute(message.id, message.args ?? []);
    if (type === "query") return session.query(message.id);
    if (type === "describe") return session.describe();
    return session.ping();
  }

  /** Posts one envelope to one window at one explicit origin. */
  function post(target, origin, envelope) {
    if (!target || typeof target.postMessage !== "function") return;
    try {
      target.postMessage({ [PROTOCOL.marker]: PROTOCOL.version, ...envelope }, origin);
    } catch {
      // A closed or detached window. Not the editor's problem, and not worth a
      // console line on every event for the rest of the session.
    }
  }

  /** Every window/origin pair an event should reach.
   *
   *  The parent first, once per allowed origin: the browser delivers the message
   *  only when the parent's real origin matches the one named, so the loop costs
   *  one delivery and drops the rest. Then any peer that has spoken to us from an
   *  allowed origin, which is how a host that is not the immediate parent (a
   *  sibling frame, an opener) still hears events. */
  function targets() {
    const out = [];
    const parent = view?.parent;
    if (parent && parent !== view) for (const origin of allowlist) out.push([parent, origin]);
    for (const [origin, source] of peers) {
      if (source === parent) continue;
      out.push([source, origin]);
    }
    return out;
  }

  function broadcast(event) {
    for (const [target, origin] of targets()) {
      post(target, origin, { kind: "event", event: event.event, detail: event.detail });
    }
  }

  // Every event the contract declares, forwarded. The list comes from the SCHEMA
  // rather than from `session.describe()`, so an added event reaches a
  // `postMessage` host with no change to this file — and so attaching the bridge
  // costs no registry build. That second part is not a micro-optimisation: the
  // bridge is attached while `main.js` is still evaluating, at a point where the
  // command registry cannot be built at all, and asking for one there would make
  // the editor's boot depend on the order two unrelated declarations sit in.
  const unsubscribes = HOST_EVENT_NAMES.map((name) => session.on(name, broadcast));

  async function onMessage(event) {
    // The origin check comes first, before the envelope is even looked at.
    if (!originAllowed(event.origin, allowlist)) return;
    const message = event.data;
    if (!isRequestEnvelope(message)) return;
    const source = event.source;
    if (source && peers.size < MAX_PEERS && !peers.has(event.origin)) {
      peers.set(event.origin, source);
    }
    const verb = verbs.get(message.type);
    let value = null;
    try {
      value = await verb(message);
    } catch (err) {
      // A verb itself failing is a bug in the editor, not a refused command — a
      // refused command is a VALUE. Reported as `bad-request` so a host still
      // gets an answer to the `rid` it is awaiting rather than a hang.
      post(source, event.origin, {
        kind: "result",
        rid: message.rid,
        ok: false,
        refusal: { code: "bad-request", command: message.id ?? null, message: String(err?.message ?? err), requires: null },
      });
      return;
    }
    // `execute` already answers with `{ok, refusal}`; `describe`, `query` and
    // `ping` answer with a value. Both shapes go back as one envelope so a host
    // has one thing to unwrap.
    const envelope =
      message.type === "execute"
        ? { kind: "result", rid: message.rid, ...value }
        : { kind: "result", rid: message.rid, ok: true, value };
    post(source, event.origin, envelope);
    // A host that connects after the editor was already ready would otherwise
    // never hear `ready` at all. Replaying it on `describe` is what makes the
    // event channel usable without a race the host has to know about.
    if (message.type === "describe" && session.ping().ready) {
      const described = value;
      post(source, event.origin, {
        kind: "event",
        event: "ready",
        detail: {
          contract: described.contract,
          capabilities: described.capabilities,
          editingMode: described.editingMode,
          commands: described.commands.length,
        },
      });
    }
  }

  const handler = (event) => void onMessage(event);
  view.addEventListener("message", handler);

  return {
    detach() {
      view.removeEventListener("message", handler);
      for (const off of unsubscribes) off();
      peers.clear();
    },
    allowlist,
    peers: () => peers.size,
  };
}
