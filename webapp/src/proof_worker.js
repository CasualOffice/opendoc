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
// ## The one subtlety: the import is dynamic, on purpose
//
// `stamp-assets.py` versions module-to-module imports through an **import map
// in each page**, because a URL resolved against `import.meta.url` drops the
// importing module's query string. Import maps do **not** apply inside a
// worker. A static `import "./proof_protocol.mjs"` here would therefore be
// fetched from a fixed URL that GitHub Pages caches for four hours — exactly
// the deploy-skew defect that script exists to prevent, one context over.
//
// So the worker reads the stamp off its OWN url (which the page did stamp, the
// same way `spell_check.mjs` stamps the dictionary fetch) and re-appends it to
// its sibling. Top-level await is correct here: a message posted before a
// module worker finishes evaluating is queued by the event loop and delivered
// after, so nothing can arrive before `responder` exists.

const stamp = new URL(self.location.href).search;
const { PROOF_MESSAGE, PROOF_PROTOCOL_VERSION, createProofResponder } = await import(
  `${new URL("./proof_protocol.mjs", import.meta.url)}${stamp}`
);

const responder = createProofResponder();

self.addEventListener("message", (event) => {
  const reply = responder.handle(event.data);
  if (reply) self.postMessage(reply);
});

// Tell the page the worker is alive and which protocol it speaks, so a page
// paired with a stale worker build falls back to in-process instead of trading
// messages neither side agrees on.
self.postMessage({ type: PROOF_MESSAGE.ready, version: PROOF_PROTOCOL_VERSION });
