// Theme, accent, and who owns them.
//
// Extracted from `main.js`'s `applySettings()` by `docs/126` phase 3, which had to
// change what that function does and had no lines to do it in. It is worth more
// than the lines: the rule "a host's brand outranks a visitor's stored
// preference" is now a plain function of two inputs, so the defect it closes is
// answerable in node instead of only in a browser with a white-labelled build
// deployed.
//
// THE DEFECT (`docs/125` §2 F4, verbatim: "Two things silently defeat
// white-labelling"). `applySettings()` ran at import and did two things to
// `:root`:
//
//   1. wrote an INLINE `--accent` from `localStorage`. An inline declaration
//      beats any author stylesheet, so a host's brand colour was overwritten at
//      boot — by the last visitor's pick, or by our own default blue for a
//      visitor who never picked anything.
//   2. REMOVED a `data-theme` attribute whenever the stored theme was `system`,
//      which is the default. F4 only mentions the write; the removal is the
//      sharper half, because a host who stamped `data-theme="dark"` on `<html>`
//      had it deleted before their first frame.
//
// HOW THE ANSWER TRAVELS. Not through a second configuration channel: through the
// stylesheet that pinned the value. `tools/build-brand.mjs` emits
// `--brand-accent-pinned: 1` into `src/brand.css` when `brand.json` set the
// accent, and this module reads it back. That is why a white-labelled build is a
// swapped static file and nothing else — no JSON to fetch, no ordering to get
// wrong, no window in which the editor was our colour and then became theirs.
//
// WHAT A HOST MAY PIN, AND WHAT IT MAY NOT. The accent, yes: it is brand.
// Light/dark, no — that is a READER's preference about their own eyes, and a host
// who took it away would be deciding how someone reads in a dark room. A host who
// genuinely wants one appearance ships the palette for it in `theme.light` and
// `theme.dark` alike, which the generator allows and the contrast audit still
// checks.

/** Whether the deployment pinned the accent, and what it is.
 *
 *  Read from the computed palette rather than from a module, because the
 *  stylesheet is the thing that actually decided. `getComputedStyle` resolves the
 *  cascade, so this answers "what is on screen", which is the only question worth
 *  asking here.
 *
 *  Complexity: O(1), called once before the first paint.
 *
 *  @param {Element} root
 *  @param {{getComputedStyle: Function}} [view]
 *  @returns {{pinned: boolean, accent: string}}
 */
export function brandAccent(root, view = globalThis) {
  let styles = null;
  try {
    styles = view.getComputedStyle(root);
  } catch {
    // A detached root, or a runtime with no layout. Fail to "not pinned", which
    // is the behaviour the editor had before this existed.
    return { pinned: false, accent: "" };
  }
  const read = (name) => styles.getPropertyValue(name).trim();
  return { pinned: read("--brand-accent-pinned") === "1", accent: read("--accent") };
}

/**
 * Puts the theme and the accent on the document root.
 *
 * `pin.pinned` is the only reason this is not two lines: a pinned accent means
 * the stylesheet already carries the right value and writing an inline one would
 * overwrite it with the visitor's.
 *
 * Complexity: O(1).
 *
 * @param {{root: Element, theme: string, accent: string, pin: {pinned: boolean}}} io
 */
export function applyAppearance({ root, theme, accent, pin }) {
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
  if (!pin.pinned) root.style.setProperty("--accent", accent);
}

/**
 * Reflects the appearance settings back into the Settings dialog.
 *
 * A pinned accent is "never, for you" for these two controls, and they are
 * DISABLED WITH A REASON rather than removed — which is the right half of
 * `docs/126`'s container policy for a control inside a surface the visitor WAS
 * offered. Word greys a policy-managed setting and says who set it rather than
 * hiding it, and a control that vanishes cannot be told from a bug.
 *
 * The swatch shows the HOST's colour when the host owns it, and no preset is
 * marked pressed: none of the six need be the host's colour, and claiming one is
 * would be Settings describing an accent that is not on screen.
 *
 * Complexity: O(swatches) — six.
 *
 * @param {object} io
 * @param {{reflect: Function}} io.themeGroup
 * @param {Element} io.swatches
 * @param {HTMLInputElement} io.custom
 * @param {{theme: string, accent: string}} io.settings
 * @param {{pinned: boolean, accent: string}} io.pin
 * @param {string} io.reason the localised sentence for a host-owned accent.
 */
export function reflectAppearance({ themeGroup, swatches, custom, settings, pin, reason }) {
  themeGroup.reflect(settings.theme);
  for (const button of swatches.querySelectorAll(".acc[data-accent]")) {
    const on = !pin.pinned && button.dataset.accent.toLowerCase() === settings.accent.toLowerCase();
    button.setAttribute("aria-pressed", String(on));
    button.disabled = pin.pinned;
    if (pin.pinned) button.title = reason;
    else button.removeAttribute("title");
  }
  // `<input type="color">` only accepts `#rrggbb`. A host palette is validated to
  // hex by the generator, but a deployment could still carry something else, and a
  // rejected value leaves the swatch showing the LAST accepted colour — which
  // would be the visitor's. So an unusable value falls back to the stored one and
  // the control still says it is not the visitor's to change.
  const shown = pin.pinned && /^#[0-9a-f]{6}$/i.test(pin.accent) ? pin.accent : settings.accent;
  custom.value = shown;
  custom.disabled = pin.pinned;
  if (pin.pinned) custom.title = reason;
  else custom.removeAttribute("title");
}
