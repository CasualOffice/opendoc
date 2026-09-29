// The phone tier's arithmetic and its cross-checks against the stylesheet
// (docs/148, ADR-044).
//
// Two kinds of test live here and they exist for different reasons.
//
// The first kind covers `keyboardInset`. A soft-keyboard calculation is the
// classic piece of code that is only ever tested by holding a phone, which
// means it is only ever tested once; the function takes plain numbers so it can
// be driven from Node instead.
//
// The second kind is a DRIFT guard, and it is the reason this file reads the
// stylesheet. `phone_chrome.mjs` declares 620 and `style.css` writes 620 into a
// media query, and nothing mechanical connects them — the same shape
// `review_layout.test.mjs` already guards for the 700px rung, for the same
// reason: a rung moved on one side only leaves a chrome that changes shape at
// one width and changes layout at another, and the gap between them is a window
// size nobody tests at.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  MIN_TOUCH_TARGET_PX,
  PHONE_MAX_WIDTH,
  REVIEW_SHEET_MAX_WIDTH,
  createPhoneChrome,
  keyboardInset,
  phoneRegions,
} from "../src/phone_chrome.mjs";

const read = (name) => readFileSync(new URL(`../src/${name}`, import.meta.url), "utf8");
const html = () => readFileSync(new URL("../editor.html", import.meta.url), "utf8");

// ---- The ladder -------------------------------------------------------------

test("the phone rung sits below the review-sheet rung", () => {
  // Not an arbitrary ordering: a tablet in portrait has no margin for a comment
  // column either, but it still has the width for a desktop chrome. If these
  // ever crossed, a window would exist that shows a phone chrome beside a
  // margin-mounted comment column, which is the layout HF-088 removed.
  assert.ok(
    PHONE_MAX_WIDTH < REVIEW_SHEET_MAX_WIDTH,
    `the phone rung (${PHONE_MAX_WIDTH}) must be narrower than the review-sheet rung (${REVIEW_SHEET_MAX_WIDTH})`,
  );
});

test("the stylesheet's phone rung is the same width the script uses", () => {
  const css = read("style.css");
  assert.match(
    css,
    new RegExp(`@media \\(max-width: ${PHONE_MAX_WIDTH}px\\)`),
    `style.css must carry a @media (max-width: ${PHONE_MAX_WIDTH}px) rung, ` +
      "or the class and the query change shape at different widths",
  );
  // And the class the script sets must actually be styled, or `phone-mode` is
  // a body class nothing reads — a whole tier that ships doing nothing, which
  // is `docs/99` §9.2's "built and unreachable" with the evidence removed.
  assert.match(css, /body\.phone-mode \{/, "style.css must define the phone chrome");
});

test("the touch-target floor the module publishes is the one the phone rung applies", () => {
  assert.equal(MIN_TOUCH_TARGET_PX, 24, "WCAG 2.5.8 Target Size (Minimum) is 24 CSS px");
  const css = read("style.css");
  const rung = css.slice(css.indexOf("body.phone-mode {"));
  // `.rail-btn` used to be the second selector here. The rail is no longer
  // painted at this rung (§5.3a), so the floor is asserted on the two surfaces
  // a phone actually taps: the navigation axis and the docked command bar.
  for (const selector of [
    "body.phone-mode .app-menu-button",
    "body.phone-mode .compact-toolbar .ctool",
  ]) {
    const rule = rung.slice(rung.indexOf(selector));
    assert.match(
      rule.slice(0, 220),
      new RegExp(`min-height: ${MIN_TOUCH_TARGET_PX}px|min-width: ${MIN_TOUCH_TARGET_PX}px`),
      `${selector} must carry the ${MIN_TOUCH_TARGET_PX}px floor`,
    );
  }
});

test("the viewport meta asks the browser to resize the layout for the keyboard", () => {
  // The script half of this is `keyboardInset`; the meta is the half that makes
  // the script return 0 on a browser that honours it, which is the good case.
  // Asserted because it is one attribute in one tag and its absence is silent:
  // the bar simply sits under the keyboard on Chrome for Android.
  const meta = html().match(/<meta name="viewport" content="([^"]+)"/);
  assert.ok(meta, "editor.html must carry a viewport meta");
  assert.match(meta[1], /interactive-widget=resizes-content/);
  assert.match(meta[1], /viewport-fit=cover/);
  // And it must NOT suppress pinch-zoom. ONLYOFFICE's mobile build does
  // (`user-scalable=no, maximum-scale=1`) because it has a canvas-level pinch
  // to put in its place; we do not, and blocking magnification without a
  // replacement is an accessibility failure, not a decision.
  assert.doesNotMatch(meta[1], /user-scalable\s*=\s*no/);
  assert.doesNotMatch(meta[1], /maximum-scale/);
});

// ---- The keyboard inset -----------------------------------------------------

test("no keyboard means no inset", () => {
  assert.equal(keyboardInset({ innerHeight: 844, visualHeight: 844, offsetTop: 0 }), 0);
});

test("an open keyboard is the difference between the two viewports", () => {
  assert.equal(keyboardInset({ innerHeight: 844, visualHeight: 508, offsetTop: 0 }), 336);
});

test("a pinch-panned visual viewport is not mistaken for a keyboard", () => {
  // Zoomed in and panned down, the visual viewport is both shorter AND offset.
  // Without subtracting the offset the bar would jump up by the pan distance
  // every time someone zoomed into a paragraph.
  assert.equal(keyboardInset({ innerHeight: 844, visualHeight: 400, offsetTop: 444 }), 0);
});

test("the inset never goes negative", () => {
  // Some browsers report a visual viewport taller than the layout one while
  // the URL bar is retracting. A negative inset would push the bar off-screen.
  assert.equal(keyboardInset({ innerHeight: 800, visualHeight: 860, offsetTop: 0 }), 0);
});

test("a browser with no visual viewport numbers reports no inset", () => {
  assert.equal(keyboardInset({ innerHeight: undefined, visualHeight: 500 }), 0);
  assert.equal(keyboardInset({ innerHeight: 800, visualHeight: NaN }), 0);
});

// ---- Regions ----------------------------------------------------------------

test("the phone withholds both navigation systems a phone has no width for", () => {
  const regions = phoneRegions();
  assert.equal(regions.ribbon, false);
  assert.equal(regions.ribbonTabs, false);
  assert.equal(regions.chromeModeToggle, false);
});

// The previous version of this test read "the rail is kept, because its Pages
// tile is the only surface that panel has", asserted `rail === true`, and
// watched `command_taxonomy.mjs` for the literal `view.pages` so that the
// exemption could not rot. It fired. This is the other side of it, and it has to
// assert the OPPOSITE implication or the exemption could come back for free:
// a region is withheld on a phone only while every capability it carries has a
// home somewhere else.
test("a region a phone withholds has no capability that lives only there", () => {
  const regions = phoneRegions();
  const taxonomy = read("command_taxonomy.mjs");

  // The rail. `#railPages` was the Pages panel's only surface; `view.pages` is
  // the home that replaced it. Delete the id from the taxonomy and this goes
  // red rather than a phone quietly losing a panel it has at 1280px.
  assert.equal(regions.rail, false);
  assert.match(
    taxonomy,
    /["']view\.pages["']/,
    "the phone withholds the rail, so #pagesPanel needs a menu home — restore " +
      "`view.pages` to APP_MENU_SECTIONS.view, or set rail:true again (docs/148 §5.3)",
  );

  // The ruler. `ruler.mjs` holds the only calls to `setTabStop`/`moveTabStop`/
  // `removeTabStop` in the product, so hiding it takes tab stops off a phone
  // unless the dialog behind `layout.tabStops` exists.
  assert.equal(regions.ruler, false);
  assert.match(
    taxonomy,
    /["']layout\.tabStops["']/,
    "the phone withholds the ruler, so tab stops need a menu home — restore " +
      "`layout.tabStops` to APP_MENU_SECTIONS.format, or set ruler:true again",
  );
});

test("the menu bar is kept, because it is the compact chrome's one axis", () => {
  assert.equal(phoneRegions().menuBar, true);
});

test("a region this table withholds is a region the stylesheet actually stops painting", () => {
  // The other way `phoneRegions()` can lie. It is DATA — nothing in `main.js`
  // reads it — so it describes what `style.css` does and is only true while the
  // stylesheet agrees. A `false` here with no rule behind it would be a design
  // document masquerading as code, which is `docs/99` §9.4's "built" that is
  // not "reachable" with the polarity flipped.
  const css = read("style.css");
  const rung = css.slice(css.indexOf("body.phone-mode {"));
  const regions = phoneRegions();
  assert.equal(regions.rail, false);
  assert.equal(regions.ruler, false);
  assert.match(
    rung,
    /body\.phone-mode \.rail,\s*\n\s*body\.phone-mode \.ruler \{\s*\n\s*display: none;/,
    "phoneRegions() withholds the rail and the ruler; style.css must be what withholds them",
  );
});

// ---- Wiring -----------------------------------------------------------------

/** A window with just enough surface to drive `createPhoneChrome`. */
function fakeView(width, { visual = null } = {}) {
  const listeners = new Map();
  return {
    innerHeight: 844,
    visualViewport: visual && {
      ...visual,
      addEventListener: () => {},
      removeEventListener: () => {},
    },
    matchMedia: (query) => {
      const limit = Number(query.match(/(\d+)px/)[1]);
      return {
        matches: width <= limit,
        addEventListener: (_type, fn) => listeners.set(limit, fn),
        removeEventListener: () => listeners.delete(limit),
      };
    },
  };
}

function fakeBody() {
  const classes = new Set();
  return {
    classes,
    classList: { toggle: (name, on) => (on ? classes.add(name) : classes.delete(name)) },
  };
}

function fakeRoot() {
  const props = new Map();
  return { props, style: { setProperty: (k, v) => props.set(k, v) } };
}

test("a phone-width window gets the phone class and a tablet one does not", () => {
  const narrow = fakeBody();
  createPhoneChrome({ view: fakeView(390), body: narrow, root: fakeRoot() });
  assert.ok(narrow.classes.has("phone-mode"));

  const wide = fakeBody();
  const chrome = createPhoneChrome({ view: fakeView(768), body: wide, root: fakeRoot() });
  assert.equal(wide.classes.has("phone-mode"), false);
  assert.equal(chrome.isPhone(), false);
  // 768 is under the review-sheet rung's 700? No — this pins that the two rungs
  // really are read from two different queries rather than from one.
  assert.equal(chrome.reviewSheet(), false);
});

test("the review-sheet rung is still answered at a width that is not a phone", () => {
  const chrome = createPhoneChrome({ view: fakeView(660), body: fakeBody(), root: fakeRoot() });
  assert.equal(chrome.isPhone(), false, "660 is wider than the phone rung");
  assert.equal(chrome.reviewSheet(), true, "660 is narrower than the review-sheet rung");
});

test("the keyboard inset is published as a custom property, and only on a phone", () => {
  const root = fakeRoot();
  createPhoneChrome({
    view: fakeView(390, { visual: { height: 508, offsetTop: 0 } }),
    body: fakeBody(),
    root,
  });
  assert.equal(root.props.get("--phone-keyboard-inset"), "336px");

  const desktop = fakeRoot();
  createPhoneChrome({
    view: fakeView(1280, { visual: { height: 508, offsetTop: 0 } }),
    body: fakeBody(),
    root: desktop,
  });
  assert.equal(
    desktop.props.get("--phone-keyboard-inset"),
    "0px",
    "a desktop window with a focused field must not inherit a keyboard inset",
  );
});

test("the header's measured height is published, and only on a phone", () => {
  // `--h-header: 63px` is a token and at this rung it is wrong: the menu bar
  // WRAPS rather than scrolling, so the header is two rows at 390px and three
  // at 320px, and a German menu bar wraps where an English one does not. The
  // toast spends this number — while a bottom sheet is open it moves to the top
  // of the screen, and the first version of that rule used the token and landed
  // the status message across "Format Table References".
  class FakeObserver {
    constructor(fn) {
      FakeObserver.last = fn;
    }
    observe() {}
    disconnect() {
      FakeObserver.disconnected = true;
    }
  }
  const header = { getBoundingClientRect: () => ({ height: 110.4 }) };

  const phone = fakeRoot();
  const view = fakeView(390);
  view.ResizeObserver = FakeObserver;
  const chrome = createPhoneChrome({ view, body: fakeBody(), root: phone, header });
  assert.equal(phone.props.get("--phone-header-height"), "110px", "rounded, in CSS pixels");

  // A re-wrap republishes it without a resize of the window.
  header.getBoundingClientRect = () => ({ height: 147 });
  FakeObserver.last();
  assert.equal(phone.props.get("--phone-header-height"), "147px");

  chrome.release();
  assert.equal(FakeObserver.disconnected, true, "the observer is released with the chrome");

  // A desktop publishes zero, so a rule that spends it inherits nothing when the
  // window grows back past the rung.
  const desktop = fakeRoot();
  const wide = fakeView(1280);
  wide.ResizeObserver = FakeObserver;
  createPhoneChrome({ view: wide, body: fakeBody(), root: desktop, header });
  assert.equal(desktop.props.get("--phone-header-height"), "0px");

  // And a browser with no `ResizeObserver` must not throw — the number is then
  // published once at startup and simply does not follow a re-wrap.
  const plain = fakeRoot();
  createPhoneChrome({ view: fakeView(390), body: fakeBody(), root: plain, header });
  assert.equal(plain.props.get("--phone-header-height"), "147px");
});

test("the stylesheet spends the inset on every region pinned to the bottom edge", () => {
  const css = read("style.css");
  const rung = css.slice(css.indexOf("body.phone-mode {"));
  // A bar that tracks the keyboard and a status strip that does not would
  // separate the moment a field took focus.
  for (const selector of [
    "body.phone-mode .compact-toolbar",
    "body.phone-mode .footer",
    "body.phone-mode .side-panel",
  ]) {
    const rule = rung.slice(rung.indexOf(selector), rung.indexOf(selector) + 600);
    assert.match(
      rule,
      /--phone-keyboard-inset|--phone-bottom-inset|--phone-bottom-chrome/,
      `${selector} must move with the keyboard`,
    );
  }
});
