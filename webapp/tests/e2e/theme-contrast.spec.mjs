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
//
// This covers the EDITOR. The site pages are swept by `site-contrast.spec.mjs`,
// which shares the same measurement (`contrast-audit.mjs`) — they used to be two
// copies of it.
import { test, expect, gotoEditor } from "./fixtures.mjs";
import { auditRegion } from "./contrast-audit.mjs";

/** How many text-bearing elements the editor chrome must present before a sweep
 *  over it means anything.
 *
 *  MEASURED at 95 in both themes, so the floor is 60. `gotoEditor` already waits
 *  for the engine to boot and the document to paint, so this is not a boot check;
 *  what it catches is the sweep silently measuring nothing — a selector that
 *  stopped matching, a chrome that did not render, a stylesheet that 404'd. A
 *  sweep over nothing reports no failures and reads exactly like a pass, which is
 *  the one way a contrast gate can be green while the product is unreadable. */
const MIN_EXAMINED = 60;

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

    const swept = await page.evaluate(auditRegion, { selector: "body" });
    expect(
      swept.examined,
      `only ${swept.examined} text elements were measured in the ${theme} theme — ` +
        "a sweep over an unpainted or unbooted editor finds no failures and passes",
    ).toBeGreaterThan(MIN_EXAMINED);
    expect(
      swept.failures.map((failure) => failure.describe),
      `unreadable text in the ${theme} theme`,
    ).toEqual([]);

    // The toast (`109` UX-017) is `hidden` at rest, and this sweep skips
    // `display: none` — so a whole new text surface, and the one that carries
    // every refusal below 620px, would have shipped unaudited. It has exactly
    // two visual states, so both are put on screen here rather than driven
    // through the app: a refusal reaches the error state, but nothing reaches
    // the plain one at desktop width, which is where the token pairing would be
    // measured.
    for (const kind of ["", "error"]) {
      await page.evaluate((k) => {
        const toast = document.getElementById("statusToast");
        toast.textContent =
          "Viewing mode is read-only; switch to Editing to change the document";
        if (k) toast.dataset.kind = k;
        else toast.removeAttribute("data-kind");
        toast.hidden = false;
        toast.classList.add("is-shown");
      }, kind);
      // `body`, not `#statusToast`: `auditRegion` walks a region's DESCENDANTS,
      // and the toast's text is its own child node, so scoping to the element
      // would audit nothing at all and pass.
      const withToast = await page.evaluate(auditRegion, { selector: "body" });
      expect(
        withToast.failures.map((failure) => failure.describe),
        `unreadable toast text (kind "${kind || "plain"}") in the ${theme} theme`,
      ).toEqual([]);
    }
  });
}
