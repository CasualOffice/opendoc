// Getting a document's faces into the engine, and saying what it cost when one
// of them could not be had.
//
// This is the browser half of the font-provisioning decision (ADR: desktop uses
// OS fonts, the browser fetches, a bundled base is always present, ONE dynamic
// registry seam serves both). `web_fonts.mjs` is the manifest — which faces
// exist, where they are served from, and what their bytes must hash to. This
// module is the PROGRAMME: when they are fetched, in what order, and what a
// reader is told.
//
// It lived in `main.js`, which is why none of the three decisions below could be
// tested without a browser and a network.
//
// ## 1. A background fetch is not the reader's feedback
//
// Both provisioning passes used to write into `#status` through `setStatus` —
// the same channel a command reports through. That channel does four things
// (`status_channel.mjs`): it paints the footer line, it writes a live region a
// screen reader speaks, it raises a toast when the line is not on screen or the
// message is a refusal, and it reports the message to an embedding host as
// editor status. So a font download spoke to a screen reader, and on the
// default editor page it OWNED the line for as long as the fetch took — which
// is seconds, because that fetch is 31.40 MB (measured below). A refusal the
// reader had just earned was overwritten by "Fetching fonts for sample.docx…".
//
// Progress now goes through the channel's `progress`, which paints the line only
// while the reader has nothing there, announces nothing, toasts nothing and
// tells the host nothing. The rule itself is in `status_policy.mjs`.
//
// ## 2. Report the loss that happened, not the fetch that failed
//
// This was wrong in the overstating direction — the same shape #795 had just
// finished fixing in the import loss reporter, which was reporting 621 losses
// that did not happen. Measured on 2026-10-06: abort the six
// named faces on the default editor page and the reader gets a persistent red
//
//     Opened sample.docx; unavailable web fonts: Roboto, Noto Sans, Noto Serif
//
// while NOTHING about the document changed — same page count, same accessibility
// text, and `caretRect` identical to the pixel at eight probe points down the
// first page. It cannot have changed: `registerFonts` registers a face BY FAMILY
// NAME and wires no coverage fallback (`casual-doc-wasm`'s `register_font`), and
// neither shipped fixture names Roboto, Noto Sans or Noto Serif anywhere — the
// sample is Calibri with Arial/Times/Courier, all of which resolve to bundled
// metric partners in `font_substitution.rs`. So the editor was reporting, in red,
// over the reader's own work, a loss that did not occur.
//
// So the arbiter is not "did a request fail" but "is anything going to paint as
// ▯". That is `uncoveredKeys` below — the buckets the document's own scalars
// asked for, minus the ones registered, minus the ones still arriving. A fetch
// that failed and left nothing uncovered cost the reader no glyph and belongs in
// the console; one that left a bucket uncovered is a real loss, named in script
// terms a reader recognises rather than as a manifest key (`109` FONT-04).
//
// Note what it is NOT: `missingCoverage()` read directly. That set is CUMULATIVE
// (`FontRegistry::note_missing` inserts and nothing clears it; its own comment
// says "so far"), so registering a face does not take its scalars back out.
// Treating it as a current state made a completely successful load report
// "Could not load the Arabic, Bengali, Devanagari, Japanese… fonts" having just
// registered every one of them — which the browser guard caught on its first
// run, and which is exactly the failure mode being fixed, in a new coat.
//
// That is also why this is not simply "stop warning": a face that IS needed and
// cannot be had must still be reported (`AGENTS.md`: no silent data loss), and
// `fontLossReport` is the one place that decides which of the two happened.
//
// ## 3. Fetch the fallbacks concurrently
//
// The buckets were fetched in a `for` loop, each awaiting the last. On the
// default editor page that is Japanese (14.27 MB), Korean (14.24 MB) and colour
// emoji (2.88 MB) end to end. They are independent downloads; only the
// `registerFallbackFont` calls have to be ordered, because each one repaginates
// and the resulting fallback chain must not depend on which request happened to
// answer first. So the fetches run together and the registrations stay in the
// document's own scalar order — the same order on every load of the same
// document. Same bytes, same chain, one round trip instead of three.

import {
  NAMED_WEB_FONT_FACES,
  SCRIPT_FALLBACK_FONTS,
  fallbackKeysFor,
  packFontBytes,
} from "./web_fonts.mjs";

/** What a fallback bucket is called in a sentence a reader reads.
 *
 * The status line used to say "Could not load the jp font", and `jp` is a key in
 * a manifest — it is not a thing anyone has. Keys without an entry fall back to
 * the key itself rather than being dropped, so adding a bucket to the manifest
 * cannot make a refusal unnameable. */
export const FALLBACK_SCRIPT_LABELS = Object.freeze({
  jp: "Japanese",
  kr: "Korean",
  sc: "Chinese",
  arabic: "Arabic",
  devanagari: "Devanagari",
  bengali: "Bengali",
  gurmukhi: "Gurmukhi",
  gujarati: "Gujarati",
  oriya: "Odia",
  tamil: "Tamil",
  telugu: "Telugu",
  kannada: "Kannada",
  malayalam: "Malayalam",
  sinhala: "Sinhala",
  hebrew: "Hebrew",
  thai: "Thai",
  symbols: "symbols",
  emoji: "emoji",
});

/** The reader-facing name of a fallback bucket. */
export function fallbackLabel(key) {
  return FALLBACK_SCRIPT_LABELS[key] ?? key;
}

/** `["Japanese", "Korean", "emoji"]` → `"Japanese, Korean and emoji"`. One
 *  comma-and list builder so a one-item and a three-item refusal read the same
 *  way. */
export function joinNames(names) {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/**
 * What the reader is told after a provisioning pass, or `null` when nothing that
 * happened is a loss they suffered.
 *
 * `uncoveredKeys` is the buckets whose scalars are going to paint as ▯ as things
 * stand — see `uncoveredKeys` in the programme below for how it is derived, and
 * why it is not `missingCoverage()` read straight. It is the whole arbiter, and
 * the reason is that it is the only one of the three inputs that describes the
 * DOCUMENT rather than the network:
 *
 *   * a bucket that failed and is still uncovered → the reader will see ▯ where
 *     their text should be, so say so and say which script;
 *   * a bucket that failed but is covered anyway → another face in the chain
 *     already draws those scalars (the bundled monochrome emoji base is exactly
 *     this case for the colour upgrade), so there is nothing to report;
 *   * a named family that failed → cannot leave a scalar uncovered on its own,
 *     because those faces are registered by family name and carry no coverage
 *     wiring. If coverage is complete the document is unaffected; if it is not,
 *     the uncovered buckets are what to name, not the family that failed.
 *
 * `kind` is `"error"` because this is a refusal: part of the document will not
 * render as itself. `null` is NOT silence — the caller still logs every failed
 * face for the console and the host.
 */
export function fontLossReport({ failedNamedFamilies = [], failedFallbackKeys = [], uncoveredKeys = [] }) {
  const uncovered = [...new Set(uncoveredKeys)];
  if (uncovered.length === 0) return null;
  const names = joinNames(uncovered.map(fallbackLabel).sort());
  const faces = uncovered.length === 1 ? "font" : "fonts";
  const blamed = [...new Set([...failedFallbackKeys, ...failedNamedFamilies])];
  return {
    text:
      `Could not load the ${names} ${faces} — that text will show as ▯ until the ` +
      `${blamed.length > 0 ? "download succeeds" : "font is provisioned"}.`,
    kind: "error",
  };
}

/** The line a background pass shows while it is fetching. Named here so the
 *  progress wording and the loss wording live together. */
export function provisioningProgress(label, keys = []) {
  const scripts = keys.length > 0 ? ` (${keys.map(fallbackLabel).join(", ")})` : "";
  return `Fetching fonts for ${label}${scripts}…`;
}

/**
 * Creates the provisioning programme over injected effects.
 *
 * Every effect is injected rather than reached for, which is what lets
 * `font_provisioning.test.mjs` drive a failed named face, a failed bucket that
 * is covered anyway and a failed bucket that is not, in node, with no browser
 * and no network — the three cases that previously required aborting a real
 * request against a real engine.
 *
 * @param {object} io
 * @param {() => object|null} io.engine the open document, or null
 * @param {(url: string) => Promise<Uint8Array>} io.fetchBytes verified font bytes
 * @param {(url: string) => void} io.releaseBytes drop a cached blob once WASM holds it
 * @param {(text: string) => void} io.progress background progress (never the reader's channel)
 * @param {(message: string, cause?: unknown) => void} io.log diagnostics for the console
 */
export function createFontProvisioning({ engine, fetchBytes, releaseBytes, progress, log }) {
  /** Buckets fetched and registered this session, so a later coverage check
   *  never re-fetches a face it already has. */
  const provisioned = new Set();
  /** Buckets whose fetch is in flight. Coverage is checked after every edit, so
   *  several checks overlap — typing three emoji fires three — and without this
   *  each would see an empty `provisioned` and start its own download of the
   *  same face. Cleared on failure so a genuine network error is retried. */
  const inFlight = new Set();

  /**
   * The engine, if it is still the same one `started` was taken from.
   *
   * EVERY engine call in this module happens after an `await` on the network,
   * and the document can be replaced while those bytes are in flight — open a
   * second `.docx` from the picker, or drop one onto the viewport, while the
   * first is still fetching its faces. `main.js` FREES the old `WasmDocument`
   * when it opens the next one, so a handle captured before the await is a freed
   * pointer and `registerFonts` / `registerFallbackFont` on it throws
   * `null pointer passed to rust`: a crash in the console, and a provisioning
   * pass that stops halfway with its buckets still marked in flight.
   *
   * Measured: `caret-alignment.spec.mjs` opens a second document while the first
   * is provisioning, and failed 5 of 5 with that error — against 5 of 5 passing
   * on `origin/main`, where the provisioning read the module-global `doc` fresh
   * at each use and so could only ever be wrong about WHICH document, never
   * about whether it still existed.
   *
   * `main.js` already states this rule in its `.then` — "a newer document may
   * have been opened while these bytes were in flight; its own provisioning owns
   * the screen, so this one must not repaint it" — but that check runs AFTER
   * this module has touched the engine. It belongs here, before the call.
   *
   * @returns the live engine, or `null` when this pass has been superseded and
   *   must abandon: the newer document runs its own pass, so there is nothing to
   *   salvage and nothing to report.
   */
  function stillCurrent(started) {
    const now = engine();
    return now && now === started ? now : null;
  }

  /** The buckets this document needs. Derived from `missingCoverage()`, which is
   *  CUMULATIVE — `FontRegistry::note_missing` inserts into a set that is never
   *  cleared, and its doc comment says "so far". So it answers "what has ever
   *  shaped to .notdef in this session", not "what is tofu now", and a registered
   *  face does not remove its scalars from it. */
  function neededKeys() {
    const doc = engine();
    return doc ? fallbackKeysFor(doc.missingCoverage()) : [];
  }

  /** The buckets whose scalars will paint as ▯ as things stand: needed, minus
   *  the ones registered, minus the ones still downloading.
   *
   *  The subtraction is the whole reason this is a function rather than
   *  `fallbackKeysFor(missingCoverage())` inline. Reading the engine's cumulative
   *  set as a current state is a mistake this module made for one iteration and
   *  the browser guard caught immediately: every bucket the document had ever
   *  wanted came back as uncovered, so a successful load reported "Could not load
   *  the Arabic, Bengali, Devanagari, Japanese… fonts" in red, having just
   *  registered all of them. In-flight is excluded for the same reason: a face
   *  that is still arriving has not been lost. */
  function uncoveredKeys() {
    return neededKeys().filter((key) => !provisioned.has(key) && !inFlight.has(key));
  }

  /** Fetches and registers the fallback buckets this document needs and has not
   *  got, returning the keys whose fetch failed. Fetches run together;
   *  registration stays in `keys` order so the fallback chain cannot depend on
   *  which request answered first. */
  async function provisionFallbacks(label) {
    const doc = engine();
    if (!doc) return [];
    const keys = uncoveredKeys();
    if (keys.length === 0) return [];
    progress(provisioningProgress(label, keys));
    for (const key of keys) inFlight.add(key);
    try {
      const settled = await Promise.allSettled(
        keys.map((key) => fetchBytes(SCRIPT_FALLBACK_FONTS[key].url)),
      );
      // The document may have been replaced and FREED while those bytes were in
      // flight. The fetched blobs stay in the byte cache deliberately: the newer
      // document's own pass will want the same buckets and will find them there.
      const live = stillCurrent(doc);
      if (!live) return [];
      const failed = [];
      for (const [index, result] of settled.entries()) {
        const key = keys[index];
        const { url, scripts } = SCRIPT_FALLBACK_FONTS[key];
        if (result.status === "rejected") {
          log(`font ${key} (${url}) failed`, result.reason);
          failed.push(key);
          continue;
        }
        // Registers + re-paginates. WASM holds the authoritative copy after
        // this, so the JS blob is released rather than double-held for the
        // tab's life.
        live.registerFallbackFont(result.value, scripts);
        provisioned.add(key);
        releaseBytes(url);
      }
      return failed;
    } finally {
      for (const key of keys) inFlight.delete(key);
    }
  }

  return {
    /**
     * The open-time pass: the named families in one batch and one repagination,
     * then whatever coverage the document's own scalars still want.
     *
     * Returns the loss report for the reader (or `null`). Every failure is
     * logged whatever the report says, so a face that failed is never invisible
     * to a developer or an embedding host even when it cost the reader nothing.
     */
    async provisionOnOpen(label) {
      const doc = engine();
      if (!doc) return null;
      progress(provisioningProgress(label));

      const named = await Promise.allSettled(
        NAMED_WEB_FONT_FACES.map((face) => fetchBytes(face.url)),
      );
      // The document may have been replaced and FREED while those ~9 MB were in
      // flight; the newer one runs its own pass, so this one abandons rather
      // than registering into a handle that no longer exists. The fetched blobs
      // stay in the byte cache deliberately: the newer pass wants the same
      // faces and will find them there.
      const live = stillCurrent(doc);
      if (!live) return null;
      const blobs = named.filter((r) => r.status === "fulfilled").map((r) => r.value);
      if (blobs.length > 0) {
        const packed = packFontBytes(blobs);
        live.registerFonts(packed.bytes, packed.lengths);
      }
      // The WASM shaper holds the authoritative copy now, so release the JS
      // byte cache (~9 MB) rather than double-holding it for the tab's life.
      for (const face of NAMED_WEB_FONT_FACES) releaseBytes(face.url);
      const failedNamedFamilies = [];
      for (const [index, result] of named.entries()) {
        if (result.status !== "rejected") continue;
        const face = NAMED_WEB_FONT_FACES[index];
        log(`font ${face.family} ${face.style} (${face.url}) failed`, result.reason);
        failedNamedFamilies.push(face.family);
      }

      const failedFallbackKeys = await provisionFallbacks(label);
      progress("");
      return fontLossReport({
        failedNamedFamilies,
        failedFallbackKeys,
        uncoveredKeys: uncoveredKeys(),
      });
    },

    /**
     * The after-an-edit pass: a just-applied edit may have introduced glyphs
     * (a checklist's ☐/☒, a pasted emoji) that no registered face covers.
     *
     * Returns `{ report, fetched }` — the loss report for the reader, and
     * whether anything new registered, which is what tells the caller to
     * re-render.
     */
    async provisionForEdit(label) {
      const before = provisioned.size;
      const failedFallbackKeys = await provisionFallbacks(label);
      const fetched = provisioned.size !== before;
      if (fetched || failedFallbackKeys.length > 0) progress("");
      return {
        fetched,
        report:
          failedFallbackKeys.length > 0
            ? fontLossReport({ failedFallbackKeys, uncoveredKeys: uncoveredKeys() })
            : null,
      };
    },
  };
}
