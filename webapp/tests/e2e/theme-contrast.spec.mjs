// Both palettes shipped text under the WCAG AA floor of 4.5:1, in different
// places, which is why neither theme could have caught the other:
//
//   * dark, at 4.42:1 — every ribbon group caption (UNDO, CLIPBOARD, FONT,
//     PARAGRAPH, STYLES, EDITING, MODE). Nine-pixel text, the smallest in the
//     product, and it was the labels naming what each toolbar group IS;
//   * light, at 4.16:1 — the search box's "⌘⇧P" hint and the footer's "Mode"
//     label.
//
// Both were the same root cause: `--faint` is readable on the surface it was
// chosen against and fails on a quieter one also used behind it. That is a
// PAIRING failure, and no amount of reviewing either token alone reveals it —
// which is why this measures the rendered app instead of auditing hex codes.
// The whole chrome is in scope, not just the ribbon, because the light failures
// were in the header and the footer.
import { test, expect, gotoEditor } from "./fixtures.mjs";

/** Runs in the page: contrast ratio of every text-bearing element in a region
 *  against the background actually composited behind it. */
const auditRegion = (selector) => {
  // Colours are resolved by the canvas parser, not by regex. Chrome hands back
  // whatever form the cascade produced — `rgb()`, `color(srgb …)`, `color-mix`,
  // and, mid-transition, `oklab()` — and a regex that assumes one of those reads
  // another's components as RGB. That is not a hypothetical: an earlier draft
  // scraped `oklab(0.49 -0.01 -0.18)` into a near-black colour and reported two
  // confident, entirely fictional failures. Canvas understands every CSS colour
  // syntax there will ever be and answers in sRGB bytes.
  const ctx = document
    .createElement("canvas")
    .getContext("2d", { willReadFrequently: true });
  const parse = (value) => {
    // An unparseable value leaves fillStyle untouched, so a sentinel is the only
    // way to tell "transparent black" from "Chrome rejected this string".
    ctx.fillStyle = "#ff00ff";
    ctx.fillStyle = value;
    if (ctx.fillStyle === "#ff00ff" && !/f0f|ff00ff|magenta/i.test(value)) {
      throw new Error(`unparseable colour: ${value}`);
    }
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
    return { r, g, b, a: a / 255 };
  };
  const luminance = (c) => {
    const channel = (v) => {
      v /= 255;
      return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
  };
  const composite = (fg, bg) => ({
    r: fg.r * fg.a + bg.r * (1 - fg.a),
    g: fg.g * fg.a + bg.g * (1 - fg.a),
    b: fg.b * fg.a + bg.b * (1 - fg.a),
    a: 1,
  });
  // Walk to the first opaque ancestor, compositing the translucent layers on the
  // way back down — a token is only as readable as whatever ends up behind it.
  const backdrop = (el) => {
    const layers = [];
    for (let n = el; n && n.nodeType === 1; n = n.parentElement) {
      const c = parse(getComputedStyle(n).backgroundColor);
      if (c.a > 0) layers.push(c);
      if (c.a === 1) break;
    }
    let acc = { r: 255, g: 255, b: 255, a: 1 };
    for (let i = layers.length - 1; i >= 0; i--) acc = composite(layers[i], acc);
    return acc;
  };
  const contrast = (a, b) => {
    const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
    return (hi + 0.05) / (lo + 0.05);
  };

  const failures = [];
  for (const el of document.querySelector(selector).querySelectorAll("*")) {
    const own = [...el.childNodes]
      .filter((n) => n.nodeType === 3)
      .map((n) => n.textContent.trim())
      .join("");
    if (!own) continue;
    const cs = getComputedStyle(el);
    if (cs.visibility === "hidden" || cs.display === "none") continue;
    const box = el.getBoundingClientRect();
    if (box.width < 1 || box.height < 1) continue;
    // Disabled controls are dimmed deliberately, and AA exempts them.
    const control = el.closest("button,select,input,textarea");
    if (control && control.disabled) continue;

    const bg = backdrop(el);
    const ratio = contrast(composite(parse(cs.color), bg), bg);
    const size = parseFloat(cs.fontSize);
    const isLarge =
      size >= 24 || (size >= 18.66 && parseInt(cs.fontWeight, 10) >= 700);
    const required = isLarge ? 3 : 4.5;
    if (ratio < required) {
      failures.push(
        `${ratio.toFixed(2)}:1 (needs ${required}:1) — "${own.slice(0, 32)}" ` +
          `<${el.tagName.toLowerCase()}.${el.className}> at ${size}px`,
      );
    }
  }
  return failures;
};

for (const theme of ["light", "dark"]) {
  test(`app chrome text meets WCAG AA in the ${theme} theme`, async ({ page }) => {
    await gotoEditor(page);

    // Measure the settled palette. Theme tokens are animated, and a colour
    // sampled mid-transition belongs to neither theme, so auditing one is
    // measuring a frame no user is expected to read.
    await page.addStyleTag({
      content: "*, *::before, *::after { transition: none !important; animation: none !important; }",
    });
    await page.evaluate(
      (t) => document.documentElement.setAttribute("data-theme", t),
      theme,
    );
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);

    const failures = await page.evaluate(auditRegion, "body");
    expect(failures, `unreadable text in the ${theme} theme`).toEqual([]);
  });
}
