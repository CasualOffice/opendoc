// Who owns the theme: the host's brand, or the visitor's stored preference.
//
// `docs/125` §2 F4 named two things that "silently defeat white-labelling", and
// both were still true when `docs/126` phase 3 started. They lived in the middle of
// `applySettings()`, a DOM reflection in a 16,000-line script, so the only thing
// that could ask about them was a browser with a white-labelled build deployed.
// They are a function of two inputs now, and this file drives them.
//
//   1. an INLINE `--accent` written on `:root` from `localStorage`. An inline
//      declaration beats any author stylesheet, so a host's brand colour was
//      overwritten at boot — by the last visitor's pick, or by our own default blue
//      for a visitor who never picked anything.
//   2. a host's `data-theme` REMOVED whenever the stored theme was `system`, which
//      is the default. F4 only mentions the write; the removal is the sharper half.
//
// The browser half is `tests/e2e/white-label.spec.mjs`: node can prove the policy,
// only a browser can prove the cascade delivers it.
import { test } from "node:test";
import assert from "node:assert/strict";

import { applyAppearance, brandAccent, reflectAppearance } from "../src/appearance.mjs";

/** A `:root` that records what was done to it. Deliberately minimal: the whole
 *  point of the extraction is that this needs no DOM. */
function fakeRoot(computed = {}) {
  const attributes = new Map();
  const inline = new Map();
  return {
    attributes,
    inline,
    style: { setProperty: (name, value) => inline.set(name, value) },
    setAttribute: (name, value) => attributes.set(name, value),
    removeAttribute: (name) => attributes.delete(name),
    getAttribute: (name) => attributes.get(name) ?? null,
    __computed: computed,
  };
}

const fakeView = (root) => ({
  getComputedStyle: (element) => ({
    getPropertyValue: (name) => element.__computed[name] ?? "",
  }),
});

test("a pinned accent is read from the stylesheet that pinned it", () => {
  // Not from a module, and not from a second configuration channel: the answer
  // travels in `src/brand.css`, which is what makes a white-labelled build a
  // swapped static file and nothing else.
  const pinned = fakeRoot({ "--brand-accent-pinned": "1", "--accent": "#0f6d6a" });
  assert.deepEqual(brandAccent(pinned, fakeView(pinned)), { pinned: true, accent: "#0f6d6a" });

  const ours = fakeRoot({ "--accent": "#3355c4" });
  assert.deepEqual(brandAccent(ours, fakeView(ours)), { pinned: false, accent: "#3355c4" });

  // Anything other than the exact marker is "not pinned": a truthy-looking value
  // must not be able to lock a deployment's visitors out of their own preference.
  for (const value of ["0", "", "true", "yes", " 1 x"]) {
    const odd = fakeRoot({ "--brand-accent-pinned": value });
    assert.equal(brandAccent(odd, fakeView(odd)).pinned, value.trim() === "1", `"${value}"`);
  }
});

test("a runtime with no layout fails to NOT pinned, which is the old behaviour", () => {
  const root = fakeRoot();
  const hostile = {
    getComputedStyle() {
      throw new Error("no layout here");
    },
  };
  assert.deepEqual(brandAccent(root, hostile), { pinned: false, accent: "" });
});

test("a host's brand accent is not overwritten by the visitor's stored one", () => {
  // F4's first half. The visitor's stored accent is a real preference and the
  // default is our blue, so this line fired for every visitor of every
  // white-labelled deployment.
  const root = fakeRoot();
  applyAppearance({ root, theme: "light", accent: "#3355c4", pin: { pinned: true } });
  assert.equal(
    root.inline.has("--accent"),
    false,
    "an inline --accent beats the host's stylesheet, so writing one un-brands the build",
  );
  // And when nobody pinned it, the visitor's preference is applied — or the
  // assertion above would pass for a function that never wrote anything.
  const free = fakeRoot();
  applyAppearance({ root: free, theme: "light", accent: "#e2622a", pin: { pinned: false } });
  assert.equal(free.inline.get("--accent"), "#e2622a");
});

test("light and dark remain the READER's to choose, pinned accent or not", () => {
  // A host may pin the accent, because that is brand. It may not pin light/dark:
  // that is a preference about someone's own eyes, and a host who took it away
  // would be deciding how a reader reads in a dark room.
  for (const pinned of [true, false]) {
    const root = fakeRoot();
    applyAppearance({ root, theme: "dark", accent: "#3355c4", pin: { pinned } });
    assert.equal(root.getAttribute("data-theme"), "dark");
    applyAppearance({ root, theme: "system", accent: "#3355c4", pin: { pinned } });
    assert.equal(root.getAttribute("data-theme"), null, "system means follow the OS");
  }
});

test("a pinned accent DISABLES its controls with a reason, and shows the host's colour", () => {
  // "Never, for you" for a CONTROL inside a surface the visitor was offered — which
  // is where "never a dead control" applies, and what Word does for a
  // policy-managed setting. A control that vanishes cannot be told from a bug.
  const swatches = fakeSwatches(["#e2622a", "#3355c4"]);
  const custom = { value: "", disabled: false, title: "", removeAttribute() { this.title = ""; } };
  const reflected = [];
  reflectAppearance({
    themeGroup: { reflect: (theme) => reflected.push(theme) },
    swatches,
    custom,
    settings: { theme: "light", accent: "#3355c4" },
    pin: { pinned: true, accent: "#0f6d6a" },
    reason: "The colour is set by the site that provides this editor.",
  });
  assert.deepEqual(reflected, ["light"]);
  assert.equal(custom.disabled, true);
  assert.equal(custom.value, "#0f6d6a", "Settings must show the accent that is on screen");
  assert.match(custom.title, /set by the site/);
  for (const button of swatches.buttons) {
    assert.equal(button.disabled, true);
    assert.match(button.title, /set by the site/);
    // No preset is pressed: none of the six need be the host's colour, and claiming
    // one is would have Settings describing an accent that is not on screen.
    assert.equal(button.attributes.get("aria-pressed"), "false");
  }
});

test("an unpinned accent leaves the controls alone, and marks the visitor's choice", () => {
  const swatches = fakeSwatches(["#e2622a", "#3355c4"]);
  const custom = { value: "", disabled: true, title: "x", removeAttribute() { this.title = ""; } };
  reflectAppearance({
    themeGroup: { reflect: () => {} },
    swatches,
    custom,
    settings: { theme: "dark", accent: "#3355C4" },
    pin: { pinned: false, accent: "" },
    reason: "unused",
  });
  assert.equal(custom.disabled, false, "the control must be re-enabled, not left as it was");
  assert.equal(custom.value, "#3355C4");
  assert.equal(custom.title, "");
  // Case-insensitively, because a colour input reports lower case and a preset is
  // authored in whatever case the markup used.
  assert.equal(swatches.buttons[1].attributes.get("aria-pressed"), "true");
  assert.equal(swatches.buttons[0].attributes.get("aria-pressed"), "false");
  assert.equal(swatches.buttons[0].disabled, false);
});

test("a host colour a colour input cannot show falls back rather than showing the wrong one", () => {
  // `<input type="color">` only accepts `#rrggbb`. A rejected value leaves the
  // swatch showing the LAST accepted colour — which would be the visitor's — so an
  // unusable value falls back explicitly, while the control still says the accent
  // is not the visitor's to change.
  const custom = { value: "#ffffff", disabled: false, title: "", removeAttribute() { this.title = ""; } };
  reflectAppearance({
    themeGroup: { reflect: () => {} },
    swatches: fakeSwatches([]),
    custom,
    settings: { theme: "light", accent: "#3355c4" },
    pin: { pinned: true, accent: "color-mix(in srgb, red 50%, blue)" },
    reason: "r",
  });
  assert.equal(custom.value, "#3355c4");
  assert.equal(custom.disabled, true);
});

/** A `#accentSwatches` that records what was done to its buttons. */
function fakeSwatches(accents) {
  const buttons = accents.map((accent) => ({
    dataset: { accent },
    disabled: false,
    title: "",
    attributes: new Map(),
    setAttribute(name, value) {
      this.attributes.set(name, value);
    },
    removeAttribute(name) {
      if (name === "title") this.title = "";
      this.attributes.delete(name);
    },
  }));
  return { buttons, querySelectorAll: () => buttons };
}
