// The engine's boot arithmetic and its retry loop (docs/109 UX-064, PROPOSAL
// D-16). The browser half — a real abort, the card, the inert chrome — is
// `e2e/boot.spec.mjs`; this is the part that can be driven from Node.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fetchWithProgress, loadEngine, progressFraction, progressTotal } from "../src/engine_boot.mjs";

test("the bar divides by the stamped raw size, else an uncompressed length, else nothing", () => {
  assert.equal(progressTotal({ hint: "28317855", contentLength: "9000000", contentEncoding: "gzip" }), 28317855);
  // gzip: Content-Length counts wire bytes, the stream yields decoded ones, so
  // dividing would reach 100% at ~45%. Unknown, said as unknown.
  assert.equal(progressTotal({ hint: null, contentLength: "9000000", contentEncoding: "gzip" }), null);
  assert.equal(progressTotal({ hint: null, contentLength: "9000000", contentEncoding: "br" }), null);
  assert.equal(progressTotal({ hint: null, contentLength: "9000000", contentEncoding: null }), 9000000);
  assert.equal(progressTotal({ hint: "", contentLength: null, contentEncoding: null }), null);
  assert.equal(progressTotal({ hint: "nope", contentLength: "12", contentEncoding: "identity" }), 12);
});

test("the fraction is clamped and unknown stays unknown", () => {
  assert.equal(progressFraction(5, 10), 0.5);
  assert.equal(progressFraction(15, 10), 1);
  assert.equal(progressFraction(5, null), null);
});

test("fetchWithProgress counts every byte through, and keeps the wasm type", async () => {
  const chunks = [new Uint8Array(3), new Uint8Array(4), new Uint8Array(5)];
  const fetchImpl = async () =>
    new Response(
      new ReadableStream({
        start(c) {
          for (const chunk of chunks) c.enqueue(chunk);
          c.close();
        },
      }),
      { headers: { "content-type": "application/wasm", "content-length": "12" } },
    );
  const seen = [];
  const response = await fetchWithProgress("x.wasm", { fetchImpl, onProgress: (l, t) => seen.push([l, t]) });
  assert.equal(response.headers.get("content-type"), "application/wasm");
  assert.equal((await response.arrayBuffer()).byteLength, 12);
  assert.deepEqual(seen.at(-1), [12, 12]);
  assert.deepEqual(seen[0], [0, 12]);
});

test("a failed load shows the card, Try again retries, and ready is called once it loads", async () => {
  const events = [];
  let attempts = 0;
  const init = async () => {
    attempts += 1;
    if (attempts < 3) throw new Error(`attempt ${attempts}`);
  };
  const boot = {
    source: () => ({ url: null, hint: null }),
    progress() {},
    fail: async (error) => events.push(`fail:${error.message}`),
    ready: () => events.push("ready"),
  };
  await loadEngine(init, boot, { onError: (e) => events.push(`log:${e.message}`) });
  assert.deepEqual(events, ["log:attempt 1", "fail:attempt 1", "log:attempt 2", "fail:attempt 2", "ready"]);
});

test("the engine is never reported ready while the failure card is waiting", async () => {
  let ready = false;
  const boot = {
    source: () => ({ url: null, hint: null }),
    progress() {},
    fail: () => new Promise(() => {}), // nobody presses Try again
    ready: () => (ready = true),
  };
  void loadEngine(async () => Promise.reject(new Error("offline")), boot);
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(ready, false);
});
