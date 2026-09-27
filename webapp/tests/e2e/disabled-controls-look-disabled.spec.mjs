// A control that ships disabled has to LOOK disabled.
//
// `SKILL.md` §10: a command that cannot run now ships "disabled with a reason,
// never as a button that does nothing". Half of that rule was being met. The
// version-history panel's "Show changes" is genuinely `disabled` and carries the
// right title — "Comparing one version with another is not built yet" — and it
// rendered IDENTICALLY to "Restore this version" beside it: same ink,
// `opacity: 1`, `cursor: pointer`, and it lit up on hover. A reason only a
// tooltip can reveal, on a control that looks live, is not a reason a user finds.
//
// The cause was that `.dialog-button` had no `:disabled` rule at all and its
// `:hover` applied regardless, so EVERY dialog button that ships disabled had
// this — seven of them in `editor.html`, not just the panel's.
//
// Found by screenshotting the panel rather than by any test, which is why this
// exists: the DOM was correct at every point, so nothing that asserts on the DOM
// could have caught it. This asserts on computed style, which is what a person
// actually sees.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

/** How the rest of the chrome already marks a disabled control — `.btn:disabled`
 *  and `.save-format:disabled` both use these. One treatment for one meaning. */
const DISABLED_MAX_OPACITY = 0.9;

test("a disabled control is visually distinct from a live one", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // CREATE THE CONDITION. The first version of this guard measured the editor's
  // default state, where the only disabled controls on screen are `.btn` ones —
  // which already had a `:disabled` rule. So it passed under a mutation that put
  // the defect straight back, which is the exact failure it exists to prevent.
  // The version-history panel is opened because it is where a `.dialog-button`
  // ships disabled with a reason: "Show changes" (diff is not built) and three
  // actions that need a selected version.
  await page.locator('[data-tab="view"]').click();
  await page.locator("#viewVersionsBtn").click();
  await expect(page.locator("#versionPanel")).toBeVisible();
  await page.waitForTimeout(400);

  const found = await page.evaluate(() => {
    const live = [];
    for (const b of document.querySelectorAll("button")) {
      if (b.offsetParent === null) continue; // not on screen; not a claim about it
      const cs = getComputedStyle(b);
      live.push({
        id: b.id || b.className.slice(0, 32),
        kind: b.className,
        disabled: b.disabled,
        opacity: Number.parseFloat(cs.opacity),
        cursor: cs.cursor,
      });
    }
    return live;
  });

  const disabled = found.filter((b) => b.disabled);
  // The half that fails when the guard breaks rather than when the chrome does:
  // a run that finds no disabled control proves nothing at all.
  expect(
    disabled.length,
    "no disabled control was on screen, so this run proves nothing",
  ).toBeGreaterThan(0);
  // Specifically: the class the defect was in. Counting any disabled button is
  // what let the first version of this guard pass under mutation.
  expect(
    disabled.filter((b) => b.kind.includes("dialog-button")).length,
    "no disabled `.dialog-button` was on screen, so this run cannot see the defect " +
      "this guard is about — the panel did not open, or its actions are all live",
  ).toBeGreaterThan(0);

  const indistinct = disabled.filter(
    (b) => b.opacity > DISABLED_MAX_OPACITY && b.cursor === "pointer",
  );
  expect(
    indistinct.map((b) => `${b.id} (opacity ${b.opacity}, cursor ${b.cursor})`),
    "these controls are disabled but render exactly like live ones — full opacity AND a " +
      "pointer cursor. A reader cannot tell they will not respond, and the reason is " +
      "reachable only by hovering for a tooltip",
  ).toEqual([]);
});
