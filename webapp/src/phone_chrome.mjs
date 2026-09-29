// The phone tier: one shell, a different set of regions (docs/148, ADR-044).
//
// WHAT THIS IS NOT. It is not a second application. ONLYOFFICE ships a whole
// second front end for a phone — `web-apps/apps/documenteditor/mobile` is a
// Framework7-React app beside the ExtJS desktop one, with its own Toolbar,
// its own Edit sheet and its own router — and the only thing the two share is
// the compiled engine (`sdkjs/common/apiBase.js:66`, `isMobileVersion` comes
// from a `mobile: true` config flag, not from a build). We deliberately do the
// opposite, because embeddability is the product: one bundle, one command
// registry, one set of guards, and a *region preset* that says which parts of
// the shell a phone paints. That is the shape `docs/137` landed on after the
// two-desktop-chromes experiment was cancelled — "not which of two layouts,
// but which regions of the one layout are present" — and `chrome_regions.mjs`
// already implements the same idea for host capability withholding.
//
// So this module owns three things and no DOM of its own:
//
//   1. **The breakpoint ladder**, in one place. `REVIEW_SHEET_MAX_WIDTH` used
//      to live in `main.js` and `PHONE_MAX_WIDTH` would have been a third
//      number in a third file. A shell with three responsive rungs spread over
//      two files and a stylesheet is how a rung gets moved on one side only.
//   2. **The `phone-mode` body class**, which is what `style.css` keys the
//      whole phone rung off. A class, not only a media query, for the reason
//      HF-088 already established for `review-sheet`: the shape changes in
//      script too (the ribbon stops being the navigation axis), and a media
//      query cannot tell `main.js` which shape it picked.
//   3. **The keyboard inset.** A bar docked to the bottom of a phone has to
//      sit on top of the soft keyboard, not under it.
//
// Everything is injected (`view`, `body`, `root`) rather than reached for, so
// this file touches no browser global and stays in `module_seams.test.mjs`'s
// `PURE_MODULES` list — the arithmetic below is unit-tested in Node against
// plain objects, which is the only way a keyboard-inset calculation ever gets
// tested at all.
//
// Complexity: every function here is O(1) and none of them reads the document.
// Per-interaction work must be O(1) in document size (`docs/107` §4) and a
// phone is the slowest device we support, so a resize handler that walked
// anything would be worse here than anywhere else.

/** The width at or below which this shell stops being a desktop.
 *
 *  620px is not a new number: it is the rung at which this chrome ALREADY
 *  drops the rail's captions, the header buttons' labels, the brand mark and
 *  the status bar's informational half. Adding a fourth breakpoint so the
 *  phone could have one of its own is exactly the "second breakpoint that only
 *  the rail knows about" that `style.css` argues against where it sheds those
 *  captions. `phone_chrome.test.mjs` asserts the stylesheet's own
 *  `@media (max-width: 620px)` literal still names this number. */
export const PHONE_MAX_WIDTH = 620;

/** The width below which the comment column stops being a margin and becomes a
 *  bottom sheet (HF-088). Above the phone rung, because a tablet in portrait
 *  has no margin to spare either but still has a desktop's chrome. Moved here
 *  from `main.js` so the two rungs are declared next to each other and can be
 *  read as one ladder. */
export const REVIEW_SHEET_MAX_WIDTH = 700;

/** WCAG 2.5.8 Target Size (Minimum), Level AA: "The size of the target for
 *  pointer inputs is at least 24 by 24 CSS pixels."
 *
 *  This repository had already converged on 24 independently in four places —
 *  `table_gutter_zones.mjs`'s `TOUCH_STRIP_PX`, `table_chrome.mjs`'s
 *  `TOUCH_PILL_PX`, and twice in `style.css` as `9px grip + 15px grow` — each
 *  restating the criterion in its own comment. One exported number is not a
 *  new rule; it is the four existing ones given a name so the fifth surface
 *  does not have to rediscover it.
 *
 *  Note that 24 is the FLOOR, not the target. Where a control floats over the
 *  document rather than sitting in a dense band, `style.css`'s
 *  `(pointer: coarse)` block already uses 44 (Apple HIG) / 48dp (Material),
 *  and that stays. */
export const MIN_TOUCH_TARGET_PX = 24;

/**
 * How far the bottom of the layout viewport is currently obscured.
 *
 * On a phone the soft keyboard does not push a fixed-position bar up. What it
 * does depends on the browser, and there are two mechanisms, so we use both:
 *
 *   * `interactive-widget=resizes-content` in the viewport meta asks the
 *     browser to shrink the LAYOUT viewport, which moves `bottom: 0` on its
 *     own. ONLYOFFICE relies on exactly this — their mobile entry point sets
 *     it and then listens for a plain `window` resize
 *     (`apps/documenteditor/mobile/src/index_dev.html:6`,
 *     `src/controller/Main.jsx:880`). They use no `visualViewport` at all.
 *   * Browsers that do not honour it shrink only the VISUAL viewport, and this
 *     function is what reads that back.
 *
 * Deliberately no threshold. An earlier draft ignored insets under ~48px on
 * the theory that a small one is a collapsing URL bar rather than a keyboard —
 * but a bar hidden behind a collapsing URL bar is just as unreachable as one
 * hidden behind a keyboard, and the threshold would have been a number with no
 * source. Whatever is covering the bottom of the window, the toolbar sits on
 * top of it.
 *
 * `offsetTop` is subtracted because a pinch-zoomed visual viewport can be
 * scrolled within the layout viewport; without it a zoomed page reports an
 * inset that is really a pan.
 *
 * @returns {number} CSS pixels, never negative.
 */
export function keyboardInset({ innerHeight, visualHeight, offsetTop = 0 }) {
  if (!Number.isFinite(innerHeight) || !Number.isFinite(visualHeight)) return 0;
  return Math.max(0, Math.round(innerHeight - visualHeight - (Number.isFinite(offsetTop) ? offsetTop : 0)));
}

/**
 * Which of the shell's regions the phone tier paints.
 *
 * Returned as data rather than applied as style so the decision is readable in
 * one place and testable without a browser — and so `docs/148` §5's table and
 * the code cannot drift, because the test compares them.
 *
 * ---- The rail and the ruler: an exemption that has now been paid off --------
 *
 * These two read `true` in the first version of this table and read `false`
 * now, and the change is worth reading, because it is the entire argument for
 * why the two command ids in `docs/148` §9 items 3 and 7 were worth building.
 *
 * `docs/148` §5.3 kept both AGAINST all three references — Google Docs, Word
 * mobile and ONLYOFFICE mobile ship neither a rail nor a ruler on a phone — for
 * one reason, stated there plainly: `#railPages` carried no `data-command`, and
 * `setTabStop` had no call site outside `ruler.mjs`. Each was therefore the
 * ONLY surface its capability had, and withholding it would have made the Pages
 * panel and every tab-stop operation unreachable at 390px while both stayed
 * reachable at 1280px. "Never a dead control" cuts both ways: a capability that
 * exists on one device class and not on another is the same defect as a button
 * that does nothing. So the rail survived on life support, and §5.3 said so and
 * said the ruler's case was weaker still.
 *
 * `view.pages` and `layout.tabStops` now exist, each with a menu home and a
 * palette row, so both regions are conveniences rather than life support and a
 * phone can spend their height on the document instead. That is ~44px of rail
 * strip and ~24px of ruler out of 844px — and the number that matters is not
 * 8% of the window but 18% of what is left once a soft keyboard has taken
 * ~300px of it.
 *
 * Note what did NOT change: neither region leaves the DOM, on a phone or
 * anywhere else. The phone tier hides regions in CSS, which is what keeps
 * `one-axis-navigation.spec.mjs`'s palette-orphan guard reading the same
 * surfaces at every width (ADR-044: "surface parity survives by construction").
 * A region that is `false` here is one a phone does not PAINT, not one the
 * shell does not HAVE.
 */
export function phoneRegions() {
  return Object.freeze({
    // Gone: the two navigation systems a desktop can afford and a phone cannot.
    ribbon: false,
    ribbonTabs: false,
    chromeModeToggle: false,
    brandMark: false,
    // Gone now that their capabilities have command homes — see above.
    rail: false,
    ruler: false,
    // Kept, restructured.
    menuBar: true,
    compactToolbar: true,
    statusBar: true,
  });
}

/**
 * Wires the phone tier to a window.
 *
 * @param {object} deps
 * @param {Window} deps.view        the window whose width and keyboard matter
 * @param {HTMLElement} deps.body   carries `phone-mode`
 * @param {HTMLElement} deps.root   carries `--phone-keyboard-inset`
 * @returns {{isPhone:()=>boolean, reviewSheet:()=>boolean,
 *            onPhoneChange:(fn:Function)=>void,
 *            onReviewSheetChange:(fn:Function)=>void, release:()=>void}}
 */
export function createPhoneChrome({ view, body, root }) {
  const media = (px) => view?.matchMedia?.(`(max-width: ${px}px)`) ?? null;
  const phoneQuery = media(PHONE_MAX_WIDTH);
  const sheetQuery = media(REVIEW_SHEET_MAX_WIDTH);
  const isPhone = () => !!phoneQuery?.matches;

  /** The inset is published as a CSS custom property rather than as a `style`
   *  on the bar, because three fixed regions need it (the toolbar, the status
   *  bar and the comment sheet) and one variable keeps them in step. */
  function syncInset() {
    const vv = view?.visualViewport;
    const px = vv
      ? keyboardInset({ innerHeight: view.innerHeight, visualHeight: vv.height, offsetTop: vv.offsetTop })
      : 0;
    root?.style?.setProperty?.("--phone-keyboard-inset", `${isPhone() ? px : 0}px`);
  }

  function syncMode() {
    body?.classList?.toggle?.("phone-mode", isPhone());
    syncInset();
  }

  syncMode();
  phoneQuery?.addEventListener?.("change", syncMode);
  // `visualViewport` is the only signal on a browser that does not honour
  // `interactive-widget`; on one that does, `innerHeight` shrinks with it and
  // this returns 0, which is correct — the layout has already moved the bar.
  view?.visualViewport?.addEventListener?.("resize", syncInset);
  view?.visualViewport?.addEventListener?.("scroll", syncInset);

  return {
    isPhone,
    reviewSheet: () => !!sheetQuery?.matches,
    onPhoneChange: (fn) => phoneQuery?.addEventListener?.("change", fn),
    onReviewSheetChange: (fn) => sheetQuery?.addEventListener?.("change", fn),
    release: () => {
      phoneQuery?.removeEventListener?.("change", syncMode);
      view?.visualViewport?.removeEventListener?.("resize", syncInset);
      view?.visualViewport?.removeEventListener?.("scroll", syncInset);
    },
  };
}
