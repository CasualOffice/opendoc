// Font provisioning for an opened document: the host's named families, then the
// script fallbacks its code points need. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { NAMED_WEB_FONT_FACES, SCRIPT_FALLBACK_FONTS, fallbackKeysFor, fetchSharedFontBytes, packFontBytes } from "./web_fonts.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createFonts(ed) {
  async function provisionFonts(name, target = ed.doc, state = ed.fontState) {
    if (!target || state.retired) return [];
    const warnings = [];
    ed.statusChannel.progress(`Fetching web fonts for ${name}…`);

    const named = await Promise.allSettled(
      NAMED_WEB_FONT_FACES.map((face) => fetchSharedFontBytes(face.url, ed.fontCache)),
    );
    const namedBytes = named
      .filter((result) => result.status === "fulfilled")
      .map((result) => result.value);
    if (namedBytes.length > 0 && !state.retired) {
      const packed = packFontBytes(namedBytes);
      target.registerFonts(packed.bytes, packed.lengths);
    }
    // The WASM shaper now holds the authoritative copy of these faces, so release
    // the JS byte cache (~9 MB) rather than double-holding it for the tab's life.
    // A subsequent document re-fetches + re-registers from the CDN as usual.
    for (const face of NAMED_WEB_FONT_FACES) ed.fontCache.delete(face.url);
    for (const [index, result] of named.entries()) {
      if (result.status === "rejected") {
        const face = NAMED_WEB_FONT_FACES[index];
        console.warn(`font ${face.family} (${face.url}) failed:`, result.reason);
        warnings.push(face.family);
      }
    }

    warnings.push(...(await provisionMissingFallbacks(name, target, state)));
    return warnings;
  }


  /** Fetches and registers any script fallback fonts the document now needs but
   *  hasn't got yet (`doc.missingCoverage()` → buckets), skipping ones already
   *  provisioned. Used both on open and after an edit that introduces new glyphs
   *  (e.g. a checklist's `☐`/`☒` markers), so newly-added symbols render instead of
   *  tofu. Returns the keys that failed to load. */
  async function provisionMissingFallbacks(label, target = ed.doc, state = ed.fontState) {
    const warnings = [];
    if (!target || state.retired) return warnings;
    const { provisionedFallbackKeys, inFlightFallbackKeys } = state;
    const missing = target.missingCoverage();
    const keys = fallbackKeysFor(missing).filter(
      (key) => !provisionedFallbackKeys.has(key) && !inFlightFallbackKeys.has(key),
    );
    if (keys.length === 0) return warnings;
    if (ed.doc === target) ed.statusChannel.progress(`Fetching fonts for ${label} (${keys.join(", ")})…`);
    for (const key of keys) inFlightFallbackKeys.add(key);
    for (const key of keys) {
      const { url, scripts } = SCRIPT_FALLBACK_FONTS[key];
      try {
        const bytes = await fetchSharedFontBytes(url, ed.fontCache);
        if (!state.retired) {
          target.registerFallbackFont(bytes, scripts); // registers + re-paginates
          provisionedFallbackKeys.add(key);
        }
        // WASM holds the authoritative copy now; drop the JS cache entry so the
        // fallback bytes are not double-held for the session (see registerFonts).
        ed.fontCache.delete(url);
      } catch (err) {
        console.warn(`font ${key} (${url}) failed:`, err);
        if (ed.doc === target) ed.setStatus(`Could not load the ${key} font — some text may show as ▯`, "error");
        warnings.push(key);
      } finally {
        inFlightFallbackKeys.delete(key);
      }
    }
    return warnings;
  }

  /** Ensures any glyphs a just-applied edit introduced (e.g. checklist checkbox
   *  markers) have a covering font, then re-renders if one was fetched. */
  async function ensureGlyphCoverage(label, target = ed.doc, state = ed.fontState) {
    if (ed.versionPreviewHome) return;
    const before = state.provisionedFallbackKeys.size;
    await provisionMissingFallbacks(label, target, state);
    if (ed.doc === target && state.provisionedFallbackKeys.size !== before) {
      await ed.renderAll();
    }
  }

  return { ensureGlyphCoverage, provisionFonts };
}
