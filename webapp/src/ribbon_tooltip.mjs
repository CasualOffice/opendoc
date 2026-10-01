// Delayed tooltips for icon-only ribbon controls (`docs/64` §3).
//
// A single custom tooltip (~350ms hover/focus delay) shows the control's name
// plus its shortcut. It reuses the existing `title`/`aria-label` content; the
// native title is suppressed only while the control is actively hovered so it
// never appears alongside the custom one, and is restored on leave — which keeps
// dynamic titles and accessibility intact.
//
// Extracted from `main.js` to pay for the chrome four already-built engine
// capabilities needed (the ¶ toggle, the measurement-unit preference, the border
// line style and Restrict Editing). `main.js` sits at its line ratchet with zero
// slack, so the two honest ways to add to it are a module or an extraction, and
// this is the extraction: nothing in here reads a single piece of editor state —
// no `doc`, no `selection`, no review mode — so it was living in the 16,000-line
// file for no reason but history. Its only inputs are an element and the keyboard
// platform, both of which arrive from `shortcut_labels.mjs` / `keyboard.mjs`
// exactly as they did before.
//
// It is NOT a pure module: it owns one `<div>` appended to the body, which is the
// thing it is. The split that matters here is state, not the DOM.
import { formatShortcut } from "./keyboard.mjs";
import { isShortcutLike } from "./shortcut_labels.mjs";

/** The controls a tooltip is offered for. Icon-only chrome, and nothing with a
 *  visible label of its own. */
export const TIP_SELECTOR = ".fmt, .ribbon-tab, .review-mode-seg, .styles-trigger";

/**
 * Installs the delayed-tooltip behaviour over one or more hover surfaces.
 *
 * ONE tooltip element for the whole chrome, created on first install, because two
 * surfaces showing two tooltips at once is the defect a shared element makes
 * unrepresentable. A `null` surface is skipped rather than refused: the ribbon
 * overflow menu does not exist in every chrome.
 *
 * Returns the controller so a caller can disarm from an event of its own — the
 * window's capturing `scroll` listener does exactly that, because a tooltip left
 * floating over the place a control used to be is worse than no tooltip.
 *
 * Complexity: O(surfaces) at install, O(1) per pointer or focus event.
 *
 * @param {Array<Element|null>} surfaces
 * @returns {{disarm: () => void}}
 */
export function installRibbonTooltips(surfaces) {
  const tooltip = document.createElement("div");
  tooltip.className = "ribbon-tooltip";
  tooltip.setAttribute("role", "tooltip");
  tooltip.hidden = true;
  document.body.append(tooltip);
  let timer = 0;
  let target = null;

  /** The name and the shortcut to print, read off the control itself. */
  function contentFor(el) {
    // The LIVE title first, the parked copy only as a fallback. `arm` removes the
    // attribute for the duration of the hover, so anything written during that
    // park — a disabled control's stated reason, above all — is NEWER than the
    // parked copy, and reading the parked one showed the stale name
    // (`docs/141` TBL-03).
    const raw = (el.getAttribute("title") || el.dataset.tipTitle || "").trim();
    const label = (el.getAttribute("aria-label") ?? "").trim();
    const match = raw.match(/^(.*?)\s*\(([^)]+)\)\s*$/);
    const own = match ? match[1] : raw;
    // A DISABLED control's title is the REASON it cannot run, and that reason is
    // the only thing worth saying about it — so it outranks the control's own
    // name here. Hovering a grey Sort button and reading "Sort rows ascending" is
    // the §10 defect itself: the tooltip is the one channel a disabled control
    // has, and it was spending it on what the button would have done.
    const name = ((el.disabled ? own : "") || label || own).trim();
    // The parenthetical is a shortcut only if it reads like one. "(3×3)" and
    // "(compact view)" are part of the name, and translating them would have
    // printed nonsense in the shortcut slot.
    const parenthetical = match ? match[2].trim() : "";
    // `isShortcutLike`, not a glyph test: the boot sweep has already rewritten
    // these titles to "Ctrl+B" on a non-Apple keyboard, and a glyph-only test
    // would then drop the chord from the tooltip on exactly the platform
    // HF-025 exists for.
    const shortcut = isShortcutLike(parenthetical) ? formatShortcut(parenthetical) : "";
    return { name, shortcut };
  }

  function position(el) {
    const rect = el.getBoundingClientRect();
    const tw = tooltip.offsetWidth;
    const th = tooltip.offsetHeight;
    let left = rect.left + rect.width / 2 - tw / 2;
    left = Math.max(6, Math.min(left, window.innerWidth - tw - 6));
    let top = rect.bottom + 6;
    if (top + th > window.innerHeight - 6) top = rect.top - th - 6;
    tooltip.style.left = `${Math.round(left)}px`;
    tooltip.style.top = `${Math.round(top)}px`;
  }

  function show(el) {
    const { name, shortcut } = contentFor(el);
    if (!name) return;
    tooltip.textContent = name;
    if (shortcut) {
      const kbd = document.createElement("kbd");
      kbd.textContent = shortcut;
      tooltip.append(kbd);
    }
    tooltip.hidden = false;
    position(el);
    tooltip.classList.add("is-visible");
  }

  function arm(el) {
    if (el.getAttribute("title")) {
      el.dataset.tipTitle = el.getAttribute("title");
      el.removeAttribute("title");
    }
    target = el;
    clearTimeout(timer);
    timer = window.setTimeout(() => {
      if (target === el) show(el);
    }, 350);
  }

  function disarm(el) {
    if (el && el.dataset.tipTitle != null) {
      // …unless something wrote a NEWER title while the attribute was parked. It
      // is removed for the whole hover, so a live `title` here is by definition
      // newer than the parked copy. Restoring the parked copy regardless is how a
      // disabled control's stated reason was silently and PERMANENTLY replaced by
      // the name of what it would have done — in every band, for every reason the
      // chrome writes, and only on controls the user had hovered, which is every
      // control they were asking about (`docs/141` TBL-03).
      if (!el.getAttribute("title")) el.setAttribute("title", el.dataset.tipTitle);
      delete el.dataset.tipTitle;
    }
    if (target === el || !el) {
      clearTimeout(timer);
      timer = 0;
      target = null;
      tooltip.classList.remove("is-visible");
      tooltip.hidden = true;
    }
  }

  for (const surface of surfaces) {
    if (!surface) continue;
    surface.addEventListener("pointerover", (e) => {
      const el = e.target.closest(TIP_SELECTOR);
      if (!el || !surface.contains(el) || el === target) return;
      if (target) disarm(target);
      arm(el);
    });
    surface.addEventListener("pointerout", (e) => {
      if (!target) return;
      if (e.relatedTarget && target.contains(e.relatedTarget)) return;
      disarm(target);
    });
    surface.addEventListener("focusin", (e) => {
      const el = e.target.closest(TIP_SELECTOR);
      if (!el) return;
      if (target && target !== el) disarm(target);
      arm(el);
    });
    surface.addEventListener("focusout", (e) => {
      const el = e.target.closest(TIP_SELECTOR);
      if (el) disarm(el);
    });
    surface.addEventListener("click", () => {
      if (target) disarm(target);
    });
  }

  return {
    disarm: () => {
      if (target) disarm(target);
    },
  };
}
