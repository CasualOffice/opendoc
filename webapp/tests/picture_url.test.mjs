// Insert ▸ Picture from URL: the validator, the format sniff, and the host's
// network policy (`picture_url.mjs`). The network is a fake, so every refusal
// is driven by the response it is about rather than by a live site.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  MAX_PICTURE_URL_BYTES,
  PictureUrlError,
  fetchPictureFromUrl,
  resolveRemoteImagePolicy,
  sniffImageType,
  validatePictureUrl,
} from "../src/picture_url.mjs";
import { PICTURE_URL_ERRORS } from "../src/picture_url.mjs";
import { readFileSync } from "node:fs";

const PNG = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
const JPEG = Uint8Array.from([0xff, 0xd8, 0xff, 0xe0, 0, 16, 0x4a, 0x46]);
const HTML = new TextEncoder().encode("<!doctype html><title>Sign in</title>");
const SVG = new TextEncoder().encode('<?xml version="1.0"?>\n<svg xmlns="http://www.w3.org/2000/svg"></svg>');

/** A fake `fetch` answering every request with `body` and `headers`, recording
 *  what it was asked. */
function fakeFetch({ body = PNG, type = "image/png", status = 200, length, url, stream = false } = {}) {
  const calls = [];
  const impl = async (requestUrl, init) => {
    calls.push({ requestUrl, init });
    const headers = new Map([["content-type", type]]);
    if (length !== undefined) headers.set("content-length", String(length));
    const chunks = stream ? [body.subarray(0, 4), body.subarray(4)] : null;
    return {
      ok: status >= 200 && status < 300,
      status,
      url: url ?? requestUrl,
      headers: { get: (name) => headers.get(name.toLowerCase()) ?? null },
      body: chunks
        ? {
            getReader() {
              let i = 0;
              return {
                read: async () => (i < chunks.length ? { done: false, value: chunks[i++] } : { done: true }),
                cancel: async () => {},
              };
            },
          }
        : null,
      arrayBuffer: async () => body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength),
    };
  };
  return { impl, calls };
}

async function refusal(promise) {
  try {
    await promise;
  } catch (err) {
    assert.ok(err instanceof PictureUrlError, `a PictureUrlError, got ${err}`);
    return err.code;
  }
  assert.fail("expected a refusal");
}

test("only http(s) addresses without credentials are accepted", () => {
  assert.deepEqual(validatePictureUrl("https://example.com/a.png"), { ok: true, url: "https://example.com/a.png" });
  assert.equal(validatePictureUrl("example.com/a.png").url, "https://example.com/a.png", "a bare host is https");
  for (const bad of ["javascript:alert(1)", "data:image/png;base64,AAAA", "file:///etc/passwd", "ftp://x/y.png", "", "https://"]) {
    assert.equal(validatePictureUrl(bad).ok, false, bad);
  }
  assert.equal(validatePictureUrl("https://me:secret@example.com/a.png").code, "credentials");
  assert.equal(validatePictureUrl(`https://example.com/${"a".repeat(3000)}`).ok, false, "over-long");
});

test("the sniff names the six formats the engine places, and SVG, and nothing else", () => {
  assert.equal(sniffImageType(PNG), "image/png");
  assert.equal(sniffImageType(JPEG), "image/jpeg");
  assert.equal(sniffImageType(new TextEncoder().encode("GIF89a....")), "image/gif");
  assert.equal(sniffImageType(Uint8Array.from([0x52, 0x49, 0x46, 0x46, 0, 0, 0, 0, 0x57, 0x45, 0x42, 0x50])), "image/webp");
  assert.equal(sniffImageType(Uint8Array.from([0x49, 0x49, 0x2a, 0x00])), "image/tiff");
  assert.equal(sniffImageType(Uint8Array.from([0x42, 0x4d, ...new Array(30).fill(0)])), "image/bmp");
  assert.equal(sniffImageType(SVG), "image/svg+xml");
  assert.equal(sniffImageType(HTML), null);
  assert.equal(sniffImageType(new Uint8Array()), null);
});

test("the host's policy: absent or direct fetches, deny refuses, a {url} template proxies, junk fails closed", () => {
  assert.deepEqual(resolveRemoteImagePolicy(null), { mode: "direct" });
  assert.deepEqual(resolveRemoteImagePolicy("DIRECT"), { mode: "direct" });
  assert.deepEqual(resolveRemoteImagePolicy("deny"), { mode: "deny" });
  assert.deepEqual(resolveRemoteImagePolicy("https://host.example/fetch?u={url}"), {
    mode: "proxy",
    template: "https://host.example/fetch?u={url}",
  });
  assert.equal(resolveRemoteImagePolicy("deny-all").mode, "deny", "a misspelling must not open the network");
  assert.equal(resolveRemoteImagePolicy("javascript:{url}").mode, "deny");
});

test("a direct fetch sends no credentials and no referrer, and embeds the bytes it sniffed", async () => {
  const net = fakeFetch({ body: JPEG, type: "image/png" });
  const got = await fetchPictureFromUrl("https://example.com/a.png", { fetchImpl: net.impl });
  assert.equal(net.calls[0].requestUrl, "https://example.com/a.png");
  assert.equal(net.calls[0].init.credentials, "omit");
  assert.equal(net.calls[0].init.referrerPolicy, "no-referrer");
  assert.equal(got.mime, "image/jpeg", "the bytes decide the format");
  assert.deepEqual([...got.bytes], [...JPEG]);
});

test("a response that is not an image is refused, by header and by bytes", async () => {
  assert.equal(await refusal(fetchPictureFromUrl("https://x.example/a", { fetchImpl: fakeFetch({ body: HTML, type: "text/html" }).impl })), "notImage");
  assert.equal(
    await refusal(fetchPictureFromUrl("https://x.example/a", { fetchImpl: fakeFetch({ body: PNG, type: "application/octet-stream" }).impl })),
    "notImage",
    "picture bytes the server does not call an image are refused too",
  );
  assert.equal(
    await refusal(fetchPictureFromUrl("https://x.example/a", { fetchImpl: fakeFetch({ body: HTML, type: "image/png" }).impl })),
    "notImage",
    "an HTML page labelled image/png is still not a picture",
  );
  assert.equal(await refusal(fetchPictureFromUrl("https://x.example/a.svg", { fetchImpl: fakeFetch({ body: SVG, type: "image/svg+xml" }).impl })), "unsupported");
});

test("a redirect to something that is not an image is refused on the FINAL response", async () => {
  const net = fakeFetch({ body: HTML, type: "text/html; charset=utf-8", url: "https://login.example/" });
  assert.equal(await refusal(fetchPictureFromUrl("https://x.example/a.png", { fetchImpl: net.impl })), "notImage");
  assert.equal(net.calls[0].init.redirect, "follow");
});

test("an oversize picture is refused from its header and, without one, mid-stream", async () => {
  assert.equal(
    await refusal(fetchPictureFromUrl("https://x.example/a.png", { fetchImpl: fakeFetch({ length: MAX_PICTURE_URL_BYTES + 1 }).impl })),
    "tooLarge",
  );
  const big = new Uint8Array(64);
  big.set(PNG);
  assert.equal(
    await refusal(fetchPictureFromUrl("https://x.example/a.png", { fetchImpl: fakeFetch({ body: big, stream: true }).impl, maxBytes: 32 })),
    "tooLarge",
    "a lying or absent Content-Length cannot buffer past the cap",
  );
});

test("the host can deny the fetch, or route it through its proxy", async () => {
  const denied = fakeFetch();
  assert.equal(await refusal(fetchPictureFromUrl("https://x.example/a.png", { policy: { mode: "deny" }, fetchImpl: denied.impl })), "denied");
  assert.equal(denied.calls.length, 0, "a denied fetch never reaches the network");
  const proxy = fakeFetch();
  await fetchPictureFromUrl("https://x.example/a b.png", {
    policy: resolveRemoteImagePolicy("https://host.example/fetch?u={url}"),
    fetchImpl: proxy.impl,
  });
  assert.equal(proxy.calls[0].requestUrl, `https://host.example/fetch?u=${encodeURIComponent("https://x.example/a%20b.png")}`);
  assert.equal(proxy.calls[0].init.credentials, "same-origin");
});

test("a network or CORS failure, an error status and a cancel each say which", async () => {
  const offline = async () => {
    throw new TypeError("Failed to fetch");
  };
  assert.equal(await refusal(fetchPictureFromUrl("https://x.example/a.png", { fetchImpl: offline })), "network");
  const notFound = await refusal(fetchPictureFromUrl("https://x.example/a.png", { fetchImpl: fakeFetch({ status: 404 }).impl }));
  assert.equal(notFound, "http");
  const abort = new AbortController();
  abort.abort();
  const aborting = async (_url, init) => {
    if (init.signal.aborted) throw Object.assign(new Error("aborted"), { name: "AbortError" });
    return null;
  };
  assert.equal(await refusal(fetchPictureFromUrl("https://x.example/a.png", { fetchImpl: aborting, signal: abort.signal })), "aborted");
});

test("every refusal code has a catalogue sentence and the dialog names each one", () => {
  const en = JSON.parse(readFileSync(new URL("../locales/en.json", import.meta.url), "utf8"));
  const dialog = readFileSync(new URL("../src/picture_url_dialog.mjs", import.meta.url), "utf8");
  for (const code of PICTURE_URL_ERRORS) {
    assert.ok(en[`pictureUrl.error.${code}`], `pictureUrl.error.${code} is in the catalogue`);
    assert.match(dialog, new RegExp(`${code}: "pictureUrl\\.error\\.${code}"`), `${code} is mapped in the dialog`);
  }
});

test("the host's policy reaches the frame as `?remoteImages=`, from the element, failing closed", async () => {
  const { hostConfig } = await import("../src/capabilities.mjs");
  const view = (search) => ({ location: { search }, self: 1, top: 1 });
  assert.equal(hostConfig(view("?remoteimages=deny")).remoteImages, "deny", "the name is case-blind");
  assert.equal(hostConfig(view("")).remoteImages, null);

  // The smallest DOM `<opendoc-editor>` touches, installed before it is imported
  // (the same shim `ai_webmcp.test.mjs` uses). Last in this file for that reason.
  const frames = [];
  globalThis.HTMLElement = class {
    constructor() { this.attributes = new Map(); this.isConnected = false; this.events = []; }
    getAttribute(k) { return this.attributes.has(k) ? this.attributes.get(k) : null; }
    hasAttribute(k) { return this.attributes.has(k); }
    setAttribute(k, v) { this.attributes.set(k, String(v)); }
    removeAttribute(k) { this.attributes.delete(k); }
    attachShadow() { return { replaceChildren() {} }; }
    dispatchEvent(e) { this.events.push(e); return true; }
  };
  globalThis.CustomEvent = class { constructor(type, init) { this.type = type; this.detail = init?.detail; } };
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  globalThis.document = {
    baseURI: "https://host.example/app/",
    createElement(tag) {
      if (tag !== "iframe") return { textContent: "" };
      const frame = { attributes: new Map(), src: "", setAttribute(k, v) { frame.attributes.set(k, v); }, addEventListener() {}, remove() {} };
      frames.push(frame);
      return frame;
    },
  };
  const warn = console.warn;
  console.warn = () => {};
  try {
    const { OpenDocEditorElement } = await import("../src/embed_element.mjs");
    assert.ok(OpenDocEditorElement.observedAttributes.includes("remote-images"), "a changed policy must remount");
    const mount = (value) => {
      const element = new OpenDocEditorElement();
      if (value !== null) element.setAttribute("remote-images", value);
      element.setAttribute("frame-title", "Editor");
      element.isConnected = true;
      element.connectedCallback();
      return { url: new URL(frames.at(-1).src), notes: element.events.at(-1).detail.notes };
    };
    const read = (value) => resolveRemoteImagePolicy(mount(value).url.searchParams.get("remoteImages"));
    assert.deepEqual(read(null), { mode: "direct" }, "absent: the standalone default");
    assert.deepEqual(read("deny"), { mode: "deny" });
    assert.deepEqual(read("https://host.example/p?u={url}"), { mode: "proxy", template: "https://host.example/p?u={url}" });
    const typo = mount("denyall");
    assert.equal(typo.url.searchParams.get("remoteImages"), "deny", "a misspelling is lowered to deny");
    assert.ok(typo.notes.some((note) => note.option === "remote-images" && note.code === "unknown"), "and is reported");
  } finally {
    console.warn = warn;
  }
});
