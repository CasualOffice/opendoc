// SPDX-License-Identifier: Apache-2.0
// THE CHROME'S ONE LENGTH: the root font size (`docs/109` HF-413).
//
// Every chrome length in `style.css` is rem, and the root is a fluid curve
// (16px across 1280-1440px, 14px at 1024px, 20px at 2560px) times
// `--chrome-scale`. So the chrome's size can change WITHOUT the window
// changing width: a host or a reader sets `--chrome-scale`, or the browser's
// own font setting moves. A script that measured the chrome once and laid out
// against that measurement — the ribbon's and the compact bar's overflow passes
// — would then keep a decision made at the old size. That is the same failure
// the font swap already taught them (`document.fonts.ready`): the GROUPS resize
// while the observed band does not, so a `ResizeObserver` on the band never
// fires.
//
// The answer here is one sentinel: an invisible 1rem x 1rem box. It resizes
// exactly when the root does, whatever moved it, and a `ResizeObserver` on it is
// the event the platform does not otherwise offer. A `(resolution: …)` query is
// watched too, for browser zoom and moving between displays of different
// density; zoom also fires `resize`, which the callers already observe.
//
// The DOCUMENT is not measured here and does not move with the root: the sheet,
// the page gap and the overlays are CSS px, and the editor's zoom control is
// what scales them.

/** The root font size in CSS px, read live. 16 when there is nothing to read
 *  (no DOM, or a value that is not a number), which is the default root.
 *  @param {Document} [doc] */
export function rootFontPx(doc = globalThis.document) {
  const root = doc?.documentElement;
  const view = doc?.defaultView;
  if (!root || typeof view?.getComputedStyle !== "function") return 16;
  const px = parseFloat(view.getComputedStyle(root).fontSize);
  return Number.isFinite(px) && px > 0 ? px : 16;
}

/** A length given in rem, in CSS px at the live root.
 *  @param {number} rem @param {Document} [doc] */
export const remToPx = (rem, doc) => rem * rootFontPx(doc);

/** Calls `fn` whenever the root font size changes, or the display resolution
 *  does (zoom, a move to another screen). The first, synchronous observation a
 *  `ResizeObserver` delivers is swallowed: the caller has already laid out at
 *  the current size, and a change is what it asked to hear about.
 *  Returns a release function.
 *  @param {() => void} fn @param {{doc?: Document}} [options] */
export function onChromeScaleChange(fn, { doc = globalThis.document } = {}) {
  const view = doc?.defaultView;
  const releases = [];
  let last = rootFontPx(doc);
  const check = () => {
    const now = rootFontPx(doc);
    if (now === last) return;
    last = now;
    fn();
  };

  if (doc?.body && typeof view?.ResizeObserver === "function") {
    const sentinel = doc.createElement("span");
    sentinel.setAttribute("aria-hidden", "true");
    sentinel.dataset.chromeScaleSentinel = "";
    // Out of flow, out of sight, out of the accessibility tree, and never a
    // hit target. Its SIZE is the point, so only that is in rem.
    sentinel.style.cssText =
      "position:fixed;left:0;top:0;width:1rem;height:1rem;visibility:hidden;" +
      "pointer-events:none;contain:strict;";
    doc.body.appendChild(sentinel);
    const observer = new view.ResizeObserver(check);
    observer.observe(sentinel);
    releases.push(() => {
      observer.disconnect();
      sentinel.remove();
    });
  }

  if (typeof view?.matchMedia === "function") {
    // `resolution` changes with zoom and with the screen; re-arm on each change,
    // because a resolution query only answers for the value it was built with.
    let query = null;
    const onResolution = () => {
      arm();
      fn();
    };
    const arm = () => {
      query?.removeEventListener?.("change", onResolution);
      query = view.matchMedia(`(resolution: ${view.devicePixelRatio || 1}dppx)`);
      query?.addEventListener?.("change", onResolution);
    };
    arm();
    releases.push(() => query?.removeEventListener?.("change", onResolution));
  }

  return () => {
    for (const release of releases.splice(0)) release();
  };
}
