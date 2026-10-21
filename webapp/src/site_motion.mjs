// The public site's motion layer: scroll reveals and illustrations that only run
// while they are on screen.
//
// PROGRESSIVE ENHANCEMENT IS THE CONTRACT, not a nicety. Every element this file
// touches is fully visible in the markup and in `marketing.css` by default. The
// hide-before-reveal rules and every looping animation are scoped under ONE class,
// `js-motion` on `<html>`, and this module is the only thing that sets it — so:
//
//   * scripts off, blocked or failed  -> no class, everything is simply there;
//   * `prefers-reduced-motion: reduce` -> the class is never set, and every
//     illustration rests on its finished frame (the base style is the final
//     state, the keyframes only ever START from a hidden one);
//   * printing                        -> `marketing.css` resets reveals in `@media print`.
//
// `site-motion.spec.mjs` holds that guarantee against the rendered pages in all
// three conditions, and was driven red by removing the scoping and by dropping
// the reduced-motion check below.
//
// Two attributes, two behaviours:
//
//   data-reveal   one-shot. Gets `is-revealed` and fades and lifts in the first
//                 time it enters the viewport, then is left alone. Children of a `data-reveal-group`
//                 are staggered through a `--reveal-i` index.
//   data-motion   continuous. Gets `is-in` while it intersects the viewport and
//                 loses it when it leaves, so an illustration's keyframes (which
//                 run only under `.is-in`) cost nothing off screen.
//
// NO LAYOUT SHIFT: a reveal moves an element with `transform` and `opacity` only,
// both compositor properties, so nothing around it reflows. And NO FLASH: anything
// already inside the first viewport is marked revealed BEFORE the class goes on,
// so a reader never sees above-the-fold content blink out and back in.
//
// Complexity: O(revealed elements) once at start; each observer callback is O(the
// entries that changed). Nothing here runs per frame.

const ROOT_CLASS = "js-motion";
/** One-shot reveals and continuous illustrations use different classes, so an
 *  element can be both without a scroll-away hiding it again. */
const REVEALED = "is-revealed";

/** Whether motion should run at all in this view. Exported for the unit test. */
export function motionAllowed(view = globalThis) {
  if (typeof view.matchMedia !== "function") return false;
  if (view.matchMedia("(prefers-reduced-motion: reduce)").matches) return false;
  return typeof view.IntersectionObserver === "function";
}

/** True when `rect` overlaps the viewport `height` at all. */
export function inFirstViewport(rect, height) {
  return rect.bottom > 0 && rect.top < height;
}

/** Stagger index for every reveal inside a group, so a row of cards arrives in
 *  reading order rather than all at once. Capped so a long group never makes its
 *  last item wait noticeably. */
function indexGroups(doc) {
  for (const group of doc.querySelectorAll("[data-reveal-group]")) {
    let i = 0;
    for (const item of group.querySelectorAll("[data-reveal]")) {
      item.style.setProperty("--reveal-i", String(Math.min(i, 6)));
      i += 1;
    }
  }
}

export function startSiteMotion(doc = globalThis.document, view = globalThis) {
  if (!doc?.documentElement || !motionAllowed(view)) return false;
  const height = view.innerHeight || doc.documentElement.clientHeight || 0;

  indexGroups(doc);
  const reveals = [...doc.querySelectorAll("[data-reveal]")];
  const pending = [];
  for (const element of reveals) {
    if (inFirstViewport(element.getBoundingClientRect(), height)) element.classList.add(REVEALED);
    else pending.push(element);
  }
  const motions = [...doc.querySelectorAll("[data-motion]")];
  for (const element of motions) {
    if (inFirstViewport(element.getBoundingClientRect(), height)) element.classList.add("is-in");
  }

  // Only now may anything be hidden: whatever the reader can already see is
  // already revealed.
  doc.documentElement.classList.add(ROOT_CLASS);

  const once = new view.IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        entry.target.classList.add(REVEALED);
        once.unobserve(entry.target);
      }
    },
    { rootMargin: "0px 0px -8% 0px", threshold: 0.12 },
  );
  for (const element of pending) once.observe(element);

  const live = new view.IntersectionObserver(
    (entries) => {
      for (const entry of entries) entry.target.classList.toggle("is-in", entry.isIntersecting);
    },
    { threshold: 0 },
  );
  for (const element of motions) live.observe(element);

  // A reader who turns reduced motion ON mid-visit gets it at once: dropping the
  // class settles every illustration on its finished frame and shows every reveal.
  const query = view.matchMedia("(prefers-reduced-motion: reduce)");
  query.addEventListener?.("change", (event) => {
    if (event.matches) doc.documentElement.classList.remove(ROOT_CLASS);
  });
  return true;
}

if (typeof document !== "undefined" && document.documentElement) {
  const start = () => {
    try {
      startSiteMotion();
    } catch {
      // Motion is decoration. A failure here must leave the page exactly as
      // authored — which, without the class, is fully visible.
      document.documentElement.classList.remove(ROOT_CLASS);
    }
  };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
}
