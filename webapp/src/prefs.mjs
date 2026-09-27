// Persisted preferences, and the guard that makes them optional.
//
// `localStorage` is HOST POLICY, not a dependency. With site data blocked, in a
// cross-origin embed, or in some private modes, even *touching*
// `window.localStorage` throws — and an unguarded module-scope read used to do
// exactly that, before the first listener was attached, leaving the whole editor
// inert: blank page, no ribbon, no keyboard. Preferences are a convenience and
// the editor is fully usable without them, so a failure here is silent by design
// and costs the session nothing but persistence.
//
// Extracted from `main.js` by `docs/126` phase 3, which needed lines in a file
// that was at its ceiling with zero slack. It is worth more than the lines: the
// storage-unavailable behaviour is now answerable in node with a fake store,
// where before it needed a browser with site data blocked
// (`tests/e2e/storage-unavailable.spec.mjs` is the only thing that could ask).
// And there is one place a preference is read or written, so no future one can be
// added without the guard — which was the original reason these were two helpers
// rather than inline calls.
//
// `view` is injected rather than reached for, which is what keeps this module out
// of the browser and testable. It defaults to `globalThis` so every existing call
// site reads unchanged.

/** The stored value for `key`, or `fallback` when storage is unavailable. */
export function readPref(key, fallback = null, view = globalThis) {
  try {
    const value = view.localStorage.getItem(key);
    return value === null ? fallback : value;
  } catch {
    return fallback;
  }
}

/** Persists `value` under `key`; a no-op when storage is unavailable. */
export function writePref(key, value, view = globalThis) {
  try {
    view.localStorage.setItem(key, value);
  } catch {
    /* private mode / storage disabled — the preference applies for this session only */
  }
}

/**
 * Loads a JSON-encoded preference object over its defaults.
 *
 * Two failures, one answer. Storage being unavailable is already `readPref`'s
 * problem; this also absorbs a CORRUPT payload — a half-written value, a value
 * written by an older build, a value someone edited in devtools — because the one
 * thing the editor must not do on a bad preference is fail to start. A settings
 * object that cannot be parsed is not a settings object, so the defaults stand.
 *
 * Complexity: O(size of the stored value).
 *
 * @param {string} key
 * @param {object} defaults
 * @param {object} [view]
 */
export function loadPrefObject(key, defaults, view = globalThis) {
  try {
    return { ...defaults, ...JSON.parse(readPref(key, "", view) || "{}") };
  } catch {
    return { ...defaults };
  }
}

/** Stores a preference object. */
export function savePrefObject(key, value, view = globalThis) {
  writePref(key, JSON.stringify(value), view);
}
