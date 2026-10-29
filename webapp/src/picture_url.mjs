// SPDX-License-Identifier: Apache-2.0
// Insert ▸ Picture ▸ From URL: the fetch, and the host's say over it.
//
// Word's Insert ▸ Pictures ▸ Online Pictures, Google Docs' Insert ▸ Image ▸
// By URL and ONLYOFFICE's Image ▸ From URL all take an address and put the
// picture in the document. Two of the three then keep a LINK to it; this
// editor never does: the bytes are downloaded once and EMBEDDED (they go into
// the media table through the same `insertImage` op as a picked file), so the
// document opens the same offline, a year later, after the site has moved.
// Local-first is the reason, and a document that silently phones a third party
// every time it is opened is the reason it matters.
//
// THE HOST DECIDES WHETHER THE EDITOR TALKS TO THE NETWORK (ADR-042 §6, the
// same seam `spell_check.mjs`'s `fetchText` gives proofing, and the consent
// shape `avatars="fetch"` gives participants' pictures, `docs/175` §8.1).
// The editor's own address carries the decision as `?remoteImages=`, which the
// `<opendoc-editor>` element writes from its `remote-images` attribute:
//
//   (absent) | "direct"  the browser fetches the address itself, with
//                        `credentials: "omit"` and no referrer, so no cookie
//                        or session of the editor's origin is sent and the
//                        site learns nothing about the document. The default,
//                        because a standalone, serverless editor has nobody
//                        else to ask — and unlike an avatar, this fetch is one
//                        the person typed and asked for.
//   "deny"               the command refuses, saying the host does not allow
//                        it. For a host whose network policy forbids it.
//   "<url with {url}>"   PROXY: the request goes to the host's own endpoint,
//                        with the picture's address percent-encoded in place of
//                        `{url}` — e.g. `https://host.example/fetch?u={url}`.
//                        Sent with `credentials: "same-origin"`, so a proxy on
//                        the editor's origin can authorise it. The host applies
//                        whatever policy it has there (allow-list, malware scan,
//                        size cap, no CORS problem at all).
//
// Anything else is unreadable and fails CLOSED, as "deny": a host that tried to
// restrict the network and misspelled it must not get the open default.
//
// WHAT IS ACCEPTED, whichever route fetched it. An http(s) address without
// embedded credentials; a response the server says is an image
// (`Content-Type: image/*`), AND whose first bytes are one of the six formats
// the engine places (`INSERTABLE_IMAGE_TYPES`, the same list Insert ▸ Picture
// and Change Picture use). Both, because either alone is a lie somebody can
// tell: a server can label an HTML error page `image/png`, and a byte sniff
// alone would accept a picture the server said was something else. A redirect
// is followed, and the FINAL response is what is checked, so a redirect from a
// picture to a login page is refused like the login page. SVG is refused: the
// engine paints no SVG of its own, and a picture it cannot paint is not one to
// put in a document. Bodies are read as a stream and abandoned the moment they
// pass `MAX_PICTURE_URL_BYTES`, so a lying or absent `Content-Length` cannot
// make the editor buffer an unbounded download.
//
// Pure: no DOM, no engine, no `t()`. Failures are `PictureUrlError`s with a
// `code`; the dialog turns a code into a sentence (`pictureUrl.error.<code>`).
import { INSERTABLE_IMAGE_TYPES } from "./picture_replace.mjs";

/** The editor-address parameter the host's decision travels in. */
export const REMOTE_IMAGES_PARAM = "remoteImages";

/** The largest picture fetched, in bytes. Larger than any photo a document
 *  needs at print resolution; small enough that the read cannot exhaust a tab. */
export const MAX_PICTURE_URL_BYTES = 20 * 1024 * 1024;

/** The longest address accepted: what every browser handles, and no more. */
export const MAX_PICTURE_URL_LENGTH = 2048;

/** Why a fetch did not produce a picture. One sentence per code in the catalogue. */
export const PICTURE_URL_ERRORS = Object.freeze([
  "invalid", // not an http(s) address
  "credentials", // user:password@ in the address
  "denied", // the host's policy refuses remote pictures
  "network", // the request failed outright: offline, DNS, or CORS
  "http", // the server answered with an error status
  "notImage", // the response is not an image, by header or by bytes
  "unsupported", // an image, but not a format the engine places (e.g. SVG)
  "tooLarge", // over MAX_PICTURE_URL_BYTES
  "empty", // no bytes
  "aborted", // the person cancelled
]);

export class PictureUrlError extends Error {
  /** @param {string} code one of `PICTURE_URL_ERRORS` @param {object} [detail] */
  constructor(code, detail = {}) {
    super(code);
    this.name = "PictureUrlError";
    this.code = code;
    this.detail = detail;
  }
}

/**
 * Reads the host's decision. O(1).
 *
 * @param {string|null|undefined} value the `remoteImages` parameter
 * @returns {{mode: "direct"} | {mode: "deny", invalid?: boolean} | {mode: "proxy", template: string}}
 */
export function resolveRemoteImagePolicy(value) {
  if (value === null || value === undefined || value === "") return { mode: "direct" };
  const text = String(value).trim();
  const lower = text.toLowerCase();
  if (lower === "direct") return { mode: "direct" };
  if (lower === "deny") return { mode: "deny" };
  if (text.includes("{url}")) {
    let parsed = null;
    try {
      parsed = new URL(text.replace("{url}", "x"));
    } catch {
      parsed = null;
    }
    if (parsed && (parsed.protocol === "https:" || parsed.protocol === "http:")) {
      return { mode: "proxy", template: text };
    }
  }
  // Unreadable: fail closed (see the header).
  return { mode: "deny", invalid: true };
}

/**
 * Validates what the person typed. Accepts a bare `example.com/a.png` as
 * https, as every browser's address bar does. O(address).
 *
 * @param {string} text
 * @returns {{ok: true, url: string} | {ok: false, code: string}}
 */
export function validatePictureUrl(text) {
  let raw = String(text ?? "").trim();
  if (!raw || raw.length > MAX_PICTURE_URL_LENGTH) return { ok: false, code: "invalid" };
  if (!/^[a-z][a-z0-9+.-]*:/i.test(raw)) raw = `https://${raw}`;
  let url;
  try {
    url = new URL(raw);
  } catch {
    return { ok: false, code: "invalid" };
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return { ok: false, code: "invalid" };
  if (!url.hostname) return { ok: false, code: "invalid" };
  if (url.username || url.password) return { ok: false, code: "credentials" };
  return { ok: true, url: url.href };
}

/**
 * The image format `bytes` really are, from their signature, or null. Only the
 * six formats the engine places are recognised; SVG is reported as
 * `image/svg+xml` so the refusal can say "unsupported" rather than "not an
 * image". O(1) — at most the first 512 bytes are read.
 *
 * @param {Uint8Array} bytes
 * @returns {string|null}
 */
export function sniffImageType(bytes) {
  const b = bytes ?? new Uint8Array();
  const at = (i, ...sig) => sig.every((v, k) => b[i + k] === v);
  if (at(0, 0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a)) return "image/png";
  if (at(0, 0xff, 0xd8, 0xff)) return "image/jpeg";
  if (at(0, 0x47, 0x49, 0x46, 0x38) && (b[4] === 0x37 || b[4] === 0x39) && b[5] === 0x61) return "image/gif";
  if (at(0, 0x42, 0x4d) && b.length >= 26) return "image/bmp";
  if (at(0, 0x49, 0x49, 0x2a, 0x00) || at(0, 0x4d, 0x4d, 0x00, 0x2a)) return "image/tiff";
  if (at(0, 0x52, 0x49, 0x46, 0x46) && at(8, 0x57, 0x45, 0x42, 0x50)) return "image/webp";
  // SVG: text, so look past a BOM, whitespace, an XML declaration or a comment.
  const head = new TextDecoder("utf-8", { fatal: false }).decode(b.subarray(0, 512)).replace(/^﻿/, "");
  if (/^\s*(<\?xml[^>]*>\s*)?(<!--[\s\S]*?-->\s*)*(<!doctype svg[^>]*>\s*)?<svg[\s>]/i.test(head)) return "image/svg+xml";
  return null;
}

/** The media type a `Content-Type` header names, lower-cased, without parameters. */
function mediaType(header) {
  return String(header ?? "").split(";")[0].trim().toLowerCase();
}

/**
 * Downloads the picture at `url` through the host's policy and returns its
 * bytes and type. Throws `PictureUrlError`.
 *
 * Complexity: O(bytes received), bounded by `maxBytes`.
 *
 * @param {string} url an address `validatePictureUrl` accepted
 * @param {object} [options]
 * @param {ReturnType<typeof resolveRemoteImagePolicy>} [options.policy]
 * @param {typeof fetch} [options.fetchImpl] the network (a test's fake, or a host's)
 * @param {AbortSignal} [options.signal] cancels the download
 * @param {number} [options.maxBytes]
 * @param {(received: number, total: number|null) => void} [options.onProgress]
 * @returns {Promise<{bytes: Uint8Array, mime: string, finalUrl: string}>}
 */
export async function fetchPictureFromUrl(url, options = {}) {
  const {
    policy = { mode: "direct" },
    fetchImpl = globalThis.fetch?.bind(globalThis),
    signal,
    maxBytes = MAX_PICTURE_URL_BYTES,
    onProgress,
  } = options;
  const checked = validatePictureUrl(url);
  if (!checked.ok) throw new PictureUrlError(checked.code);
  if (policy.mode === "deny" || typeof fetchImpl !== "function") throw new PictureUrlError("denied");
  const proxied = policy.mode === "proxy";
  const requestUrl = proxied ? policy.template.replace("{url}", encodeURIComponent(checked.url)) : checked.url;
  let response;
  try {
    response = await fetchImpl(requestUrl, {
      method: "GET",
      credentials: proxied ? "same-origin" : "omit",
      referrerPolicy: "no-referrer",
      redirect: "follow",
      cache: "no-store",
      signal,
    });
  } catch (err) {
    if (signal?.aborted || err?.name === "AbortError") throw new PictureUrlError("aborted");
    // A CORS refusal and an unreachable host are indistinguishable to a page by
    // design (the browser will not say which); the sentence names both.
    throw new PictureUrlError("network");
  }
  if (!response?.ok) throw new PictureUrlError("http", { status: response?.status ?? 0 });
  // The FINAL response's header: a redirect from a picture to a page is a page.
  const declared = mediaType(response.headers?.get?.("content-type"));
  if (!declared.startsWith("image/")) throw new PictureUrlError("notImage");
  const lengthHeader = Number(response.headers?.get?.("content-length"));
  const total = Number.isFinite(lengthHeader) && lengthHeader > 0 ? lengthHeader : null;
  if (total !== null && total > maxBytes) throw new PictureUrlError("tooLarge");
  const bytes = await readBounded(response, { maxBytes, total, signal, onProgress });
  if (!bytes.length) throw new PictureUrlError("empty");
  const sniffed = sniffImageType(bytes);
  if (sniffed === null) throw new PictureUrlError("notImage");
  // The bytes decide the format (a server that says PNG and sends JPEG is
  // common and harmless); they may not decide that it is an image at all.
  if (!INSERTABLE_IMAGE_TYPES.has(sniffed)) throw new PictureUrlError("unsupported");
  return { bytes, mime: sniffed, finalUrl: response.url || checked.url };
}

/** Reads `response`'s body as a stream, refusing past `maxBytes`. */
async function readBounded(response, { maxBytes, total, signal, onProgress }) {
  const reader = response.body?.getReader?.();
  if (!reader) {
    // No stream (an old engine, or a fake): one buffered read, still bounded.
    let buffer;
    try {
      buffer = new Uint8Array(await response.arrayBuffer());
    } catch {
      throw new PictureUrlError(signal?.aborted ? "aborted" : "network");
    }
    if (buffer.length > maxBytes) throw new PictureUrlError("tooLarge");
    onProgress?.(buffer.length, total);
    return buffer;
  }
  const chunks = [];
  let received = 0;
  for (;;) {
    let step;
    try {
      step = await reader.read();
    } catch {
      throw new PictureUrlError(signal?.aborted ? "aborted" : "network");
    }
    if (step.done) break;
    received += step.value.length;
    if (received > maxBytes) {
      reader.cancel?.().catch?.(() => {});
      throw new PictureUrlError("tooLarge");
    }
    chunks.push(step.value);
    onProgress?.(received, total);
  }
  const out = new Uint8Array(received);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}
