// The engine's boot, designed (docs/109 UX-064, PROPOSAL D-16; audit FILE-02,
// FILE-03, SHELL-08, MOBILE-07).
//
// Before this, the ~13 MB (gzipped) engine downloaded behind a 13px "Loading
// engine…" in the status bar, the ribbon looked usable the whole time and
// answered nothing, and a failed download was one red status line with no way
// to retry. Docs and Word web paint a skeleton of the final layout with a
// progress signal; ONLYOFFICE shows a loader mask with a percentage over a
// toolbar laid out at its final size, and a modal with Reload when it fails.
// This module is that, in three parts:
//
//   1. `fetchWithProgress` — the wasm response, re-wrapped so every chunk is
//      counted on its way into `WebAssembly.instantiateStreaming`. It is still
//      ONE streaming compile of ONE download: the bytes pass through a
//      `TransformStream`, they are not buffered, and the wrapped response keeps
//      the `application/wasm` type the streaming compile requires. The request
//      is the same URL, mode and credentials as the `<link rel=preload>` in
//      editor.html, so the browser hands over the preloaded response rather
//      than fetching twice.
//   2. `createEngineBoot` — the splash's progress bar, the failure card with
//      Try again and Details, and the chrome's `inert` (set in markup, lifted
//      here when the engine is in).
//   3. `loadEngine` — the retry loop: a failure shows the card and waits for
//      Try again, which re-fetches and re-instantiates in place. wasm-bindgen's
//      `init` only caches on success, so a second call is a clean second try.
//      In place rather than `location.reload()`: a reload would re-download the
//      ~500 KB module graph that did arrive, and lose the URL's transient state.
//
// What this does NOT cover, said here so it is not read as covered: a failure
// to load the JS module graph itself (the glue or main.js) never reaches this
// code, because it is the code. That leaves the page on its skeleton, inert.
//
// Complexity: O(1) per chunk; nothing here reads the document.

/** The byte total a progress bar can honestly divide by, or null.
 *
 *  `Content-Length` counts the bytes ON THE WIRE. When the response is
 *  compressed (GitHub Pages and `serve.py` both gzip the wasm) the stream a
 *  script reads yields the DECOMPRESSED bytes, roughly 2.2x more, so dividing
 *  one by the other would race to 100% at 45% and then sit there. The deploy
 *  stamps the raw size onto the preload link (`stamp-assets.py`, `data-bytes`),
 *  which is the right denominator; without it, an uncompressed length is still
 *  usable, and anything else is unknown, and the bar says so by being
 *  indeterminate rather than by guessing. */
export function progressTotal({ hint, contentLength, contentEncoding }) {
  const stamped = Number(hint);
  if (Number.isFinite(stamped) && stamped > 0) return stamped;
  const encoding = (contentEncoding || "identity").trim().toLowerCase();
  const length = Number(contentLength);
  if (encoding === "identity" && Number.isFinite(length) && length > 0) return length;
  return null;
}

/** A fraction in [0, 1] for `loaded` of `total`, or null when the total is
 *  unknown. Clamped, because a stamped size from an older build can be smaller
 *  than what arrives. */
export function progressFraction(loaded, total) {
  if (!total) return null;
  return Math.max(0, Math.min(1, loaded / total));
}

/**
 * Fetches `url` and resolves to a `Response` whose body reports its progress.
 *
 * @param {string|URL} url
 * @param {object} options
 * @param {(loaded:number, total:number|null)=>void} options.onProgress
 * @param {number|string|null} [options.hint] the raw byte size, when known
 * @param {typeof fetch} [options.fetchImpl]
 * @returns {Promise<Response>}
 */
export async function fetchWithProgress(url, { onProgress, hint = null, fetchImpl = globalThis.fetch }) {
  const response = await fetchImpl(url, { credentials: "same-origin" });
  if (!response.ok || !response.body || typeof TransformStream !== "function") return response;
  const total = progressTotal({
    hint,
    contentLength: response.headers.get("content-length"),
    contentEncoding: response.headers.get("content-encoding"),
  });
  let loaded = 0;
  onProgress?.(0, total);
  const counter = new TransformStream({
    transform(chunk, controller) {
      loaded += chunk.byteLength;
      onProgress?.(loaded, total);
      controller.enqueue(chunk);
    },
  });
  // Headers are copied whole: `instantiateStreaming` refuses a response whose
  // Content-Type is not `application/wasm`.
  return new Response(response.body.pipeThrough(counter), {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers,
  });
}

/**
 * Binds the splash, the failure card and the inert chrome in `doc`.
 *
 * Every element is optional — a host that strips the splash or the card gets a
 * boot that still works and still lifts `inert` (`chrome_regions.mjs`' rule for
 * a cut-down shell: an absent region is not a throw).
 *
 * @param {Document} doc
 */
export function createEngineBoot(doc) {
  const root = doc.documentElement;
  const splash = doc.getElementById("bootSplash");
  const bar = doc.getElementById("bootProgress");
  const percent = doc.getElementById("bootPercent");
  const card = doc.getElementById("bootFailure");
  const detail = doc.getElementById("bootFailureDetail");
  const retry = doc.getElementById("bootRetry");
  const numberFormat = new Intl.NumberFormat(root.lang || undefined, { style: "percent", maximumFractionDigits: 0 });

  // Repainting on every chunk would be ~1,000 style writes for a 28 MB binary;
  // one per animation frame is all a person can see.
  let pending = null;
  let frame = 0;
  const raf = doc.defaultView?.requestAnimationFrame?.bind(doc.defaultView) ?? ((fn) => setTimeout(fn, 16));
  function paint() {
    frame = 0;
    if (!pending || !bar) return;
    const fraction = progressFraction(pending.loaded, pending.total);
    if (fraction === null) {
      bar.removeAttribute("aria-valuenow");
      bar.dataset.state = "indeterminate";
      if (percent) percent.textContent = "";
      return;
    }
    const value = Math.round(fraction * 100);
    bar.dataset.state = "determinate";
    bar.setAttribute("aria-valuenow", String(value));
    bar.style.setProperty("--boot-progress", String(fraction));
    if (percent) percent.textContent = numberFormat.format(fraction);
  }

  return {
    /** The wasm URL the page preloads — the one the glue would fetch — and the
     *  stamped raw size, or nulls when the markup has no preload. */
    source() {
      const link = doc.querySelector('link[rel="preload"][href*="casual_doc_wasm_bg.wasm"]');
      return { url: link?.href ?? null, hint: link?.dataset?.bytes ?? null };
    },
    progress(loaded, total) {
      pending = { loaded, total };
      if (!frame) frame = raf(paint);
    },
    /** Shows the card and resolves when the person presses Try again. */
    fail(error) {
      root.classList.add("engine-failed");
      if (detail) detail.textContent = String(error?.message ?? error ?? "");
      if (splash) splash.hidden = true;
      if (!card || !retry) return new Promise(() => {});
      card.hidden = false;
      retry.focus({ preventScroll: true });
      return new Promise((resolve) => {
        retry.addEventListener(
          "click",
          () => {
            card.hidden = true;
            root.classList.remove("engine-failed");
            if (splash) splash.hidden = false;
            pending = { loaded: 0, total: null };
            paint();
            resolve();
          },
          { once: true },
        );
      });
    },
    /** The engine is instantiated: the chrome becomes live. The splash stays up
     *  until the document lands (`html.doc-expected body:not(.doc-loaded)`
     *  keeps it), now saying it is opening the document. */
    ready() {
      for (const el of doc.querySelectorAll("[data-boot-inert]")) {
        el.inert = false;
        el.removeAttribute("data-boot-inert");
      }
      root.classList.remove("engine-pending", "engine-failed");
      root.classList.add("engine-ready");
      if (card) card.hidden = true;
    },
  };
}

/**
 * Instantiates the engine, retrying through the failure card until it loads.
 *
 * @param {(options?:object)=>Promise<unknown>} init wasm-bindgen's default export
 * @param {ReturnType<typeof createEngineBoot>} boot
 * @param {object} [options]
 * @param {typeof fetch} [options.fetchImpl]
 * @param {(error:unknown)=>void} [options.onError]
 */
export async function loadEngine(init, boot, { fetchImpl = globalThis.fetch, onError = () => {} } = {}) {
  for (;;) {
    try {
      const { url, hint } = boot.source();
      if (url && typeof fetchImpl === "function") {
        await init({ module_or_path: fetchWithProgress(url, { hint, fetchImpl, onProgress: (l, t) => boot.progress(l, t) }) });
      } else {
        await init();
      }
      boot.ready();
      return;
    } catch (error) {
      onError(error);
      await boot.fail(error);
    }
  }
}
