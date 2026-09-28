// The proofing worker: a transport over `proof_protocol.mjs`, and nothing else.
//
// `docs/146` §4, ADR-042. Every decision a finding depends on is in the
// protocol module, which is pure; this file exists only so that those decisions
// run on a thread that is not the one painting the document. If you are about
// to add a rule, a dictionary format or a ranking tweak here, it belongs in
// `proof_protocol.mjs` — the in-process fallback in `spell_check.mjs` calls the
// same responder, and anything added here would silently not apply to it.
//
// It is `.js` rather than `.mjs` for the reason `module_seams.test.mjs` states:
// `.mjs` is a module something imports, `.js` is an entry script that runs for
// its side effects and exports nothing. A worker entry is the second kind.
//
// ## Two subtleties, and the second one cost a debugging session
//
// **The import is dynamic.** `stamp-assets.py` versions module-to-module
// imports through an **import map in each page**, because a URL resolved
// against `import.meta.url` drops the importing module's query string. Import
// maps do **not** apply inside a worker. A static `import "./proof_protocol.mjs"`
// here would therefore be fetched from a fixed URL that GitHub Pages caches for
// four hours — exactly the deploy-skew defect that script exists to prevent,
// one context over. So the worker reads the stamp off its OWN url (which the
// page did stamp, the same way `spell_check.mjs` stamps the dictionary fetch)
// and re-appends it to its sibling.
//
// **The listener is registered BEFORE that import, and early messages are
// QUEUED.** This file first used top-level `await` for the import, on the
// reasoning that a message posted before a module worker finishes evaluating is
// queued by the event loop and delivered after. That is true of the initial
// synchronous evaluation — and top-level await ends it. Once the module yields
// at the first `await`, the worker starts dispatching queued messages, and
// there is no `message` listener yet, so they are **dropped on the floor**.
//
// The message that gets dropped is the first one, which is the one carrying the
// word list. Nothing errors, nothing warns: the worker then answers every later
// check correctly, with grammar findings only, for ever, because the resources
// are sent once. Measured in the browser suite as "the document is never
// spell-checked" in roughly half of a two-worker run, with a healthy worker and
// a clean console. So: the listener goes on in the FIRST synchronous statement,
// and anything that arrives before the protocol is loaded waits in `queued`.

/** Messages that arrived before the protocol module finished loading. */
const queued = [];
let responder = null;

function deliver(message) {
  const reply = responder.handle(message);
  if (reply) self.postMessage(reply);
}

self.addEventListener("message", (event) => {
  if (!responder) {
    queued.push(event.data);
    return;
  }
  deliver(event.data);
});

const stamp = new URL(self.location.href).search;
import(`${new URL("./proof_protocol.mjs", import.meta.url)}${stamp}`)
  .then(({ PROOF_MESSAGE, PROOF_PROTOCOL_VERSION, createProofResponder }) => {
    responder = createProofResponder();
    while (queued.length) deliver(queued.shift());
    // Only now: the page takes `ready` as "everything I have sent has been
    // seen", and it is only true once the backlog has been drained.
    self.postMessage({ type: PROOF_MESSAGE.ready, version: PROOF_PROTOCOL_VERSION });
  })
  .catch(() => {
    // A version the page cannot speak, so it falls back to checking in process
    // rather than waiting for a worker that will never answer. Literals,
    // because the module that defines the constants is the one that failed.
    self.postMessage({ type: "proof:ready", version: 0 });
  });
