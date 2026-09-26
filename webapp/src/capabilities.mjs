// What this editor is allowed to be, decided before the first paint.
//
// A framed editor is currently a STANDALONE editor: File ▸ New and File ▸ Open
// are live inside someone else's page, so a visitor can replace the host's
// document from within the host's own chrome. opencalc measured and fixed
// exactly this defect (`docs/104` §HF-109), and `docs/125` §2 F3 records it
// here — along with the two places that already know about framing and simply
// never generalised: autosave defaults off when framed, and one command already
// says "Autosave is off in an embedded editor".
//
// This is the cheap half of `docs/125`'s phase 2: presets resolved before first
// paint, no SDK required. The expensive half — enforcing per operation at the
// engine so a permission cannot be defeated from devtools (`docs/104` D-5) —
// is deliberately NOT here. What this file decides is what the CHROME offers.
// A capability absent from the set must therefore disable a control with a
// reason, never merely hide it, or the product lies about what it can do.
//
// Pure: no DOM, no engine. The host inputs arrive as arguments so the whole
// resolution is answerable in node.

/** Everything a host can grant or withhold. Deliberately small — this is the
 *  v1 set `docs/125` §5.2 names, not a wish list. */
export const CAPABILITIES = Object.freeze([
  "open", // replace the loaded document from inside the editor
  "new", // start a blank document
  "save", // write the document back out
  "download", // export to a file the visitor keeps
  "print",
  "edit", // mutate the document at all
  "comment",
  "branding", // show OUR name and mark
]);

/** The three presets. A host names one; the fourth state is "no host, no
 *  question" — a page opened directly, which is `standalone`. */
const PRESETS = Object.freeze({
  standalone: Object.freeze([...CAPABILITIES]),
  // Framed inside someone else's application. It may still edit and comment —
  // that is why it was embedded — but it may not reach for a DIFFERENT
  // document, and it does not advertise us inside their product.
  embedded: Object.freeze(["save", "download", "print", "edit", "comment"]),
  // A reader. Nothing that changes the document, nothing that leaves with it.
  viewer: Object.freeze(["print"]),
});

/** The preset names a host may ask for, for validation and for tests. */
export const PRESET_NAMES = Object.freeze(Object.keys(PRESETS));

/**
 * Resolves the capability set for this page load.
 *
 * `mode` is the host's explicit request (`?mode=embedded`). When absent, being
 * FRAMED decides it: a page that is not the top window was put there by someone
 * else, and the safe reading of that is `embedded`, not `standalone`. That
 * default is the whole point — it closes the defect without a host having to
 * know this parameter exists.
 *
 * An unrecognised `mode` falls back to the framed-derived default rather than
 * throwing: a typo in a host's URL must not leave the editor with no chrome at
 * all, and it must not silently grant MORE than the default either.
 *
 * Complexity: O(1).
 */
export function resolveCapabilities({ mode = null, framed = false } = {}) {
  const asked = typeof mode === "string" ? mode.trim().toLowerCase() : "";
  const preset = PRESETS[asked] ?? PRESETS[framed ? "embedded" : "standalone"];
  return new Set(preset);
}

/** Reads the two host inputs off a real page. Separated from `resolveCapabilities`
 *  so the decision itself stays pure and testable. */
export function hostCapabilities(view = globalThis) {
  const search = view?.location?.search ?? "";
  let mode = null;
  try {
    mode = new URLSearchParams(search).get("mode");
  } catch {
    mode = null;
  }
  // `window.self !== window.top` throws on a cross-origin parent in some
  // engines; a throw means we ARE framed, which is the safer answer anyway.
  let framed = false;
  try {
    framed = view.self !== view.top;
  } catch {
    framed = true;
  }
  return resolveCapabilities({ mode, framed });
}
