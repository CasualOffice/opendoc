import { expect, gotoEditor, test, runPaletteCommand, clickIntoFirstPage } from "./fixtures.mjs";
import { DIALOGS } from "./modal-roster.mjs";

const OUT = "/private/tmp/claude-501/-Users-sachin-Desktop-melp-services-opendoc/371a889a-0017-478b-8581-29a91f1fb667/scratchpad/ui-v2-shots/audit-after";
const ONLY = (process.env.SHOTS ?? "").split(",").filter(Boolean);

for (const dialog of DIALOGS()) {
  if (ONLY.length && !ONLY.includes(dialog.id)) continue;
  for (const [vw, vh, theme] of [[1280, 800, "light"], [1280, 800, "dark"], [390, 844, "light"]]) {
    test(`shot ${dialog.id} ${vw} ${theme}`, async ({ page }) => {
      await page.setViewportSize({ width: vw, height: vh });
      if (vw < 600) {
        await gotoEditor(page);
        await page.evaluate((id) => { document.getElementById(id).hidden = false; document.body.classList.add("modal-open"); }, dialog.id);
      } else {
        await dialog.open(page);
      }
      await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
      await page.waitForTimeout(300);
      await page.screenshot({ path: `${OUT}/dialog-${dialog.id}-${vw}-${theme}.png` });
    });
  }
}

const PANELS = [
  ["outlinePanel", async (page) => page.locator("#railOutline").click()],
  ["pagesPanel", async (page) => page.locator("#railPages").click()],
  ["versionPanel", async (page) => page.locator("#railVersions").click()],
  ["comparePanel", async (page) => page.locator("#railCompare").click()],
  ["paragraphPropertiesPanel", async (page) => { await clickIntoFirstPage(page); await page.locator("#paraOptsBtn").click(); }],
  ["symbolDialog", async (page) => { await page.locator('[data-tab="insert"]').click(); await page.locator("#insertSymbolBtn").click(); }],
];
if (!ONLY.length || ONLY.includes("panels")) {
  for (const [id, open] of PANELS) {
    for (const [vw, vh, theme] of [[1280, 800, "light"], [1280, 800, "dark"], [390, 844, "light"]]) {
      test(`shot panel ${id} ${vw} ${theme}`, async ({ page }) => {
        await page.setViewportSize({ width: vw, height: vh });
        await gotoEditor(page, "?fixture=rich");
        if (vw < 600) await page.evaluate((i) => { document.getElementById(i).hidden = false; }, id);
        else await open(page);
        await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
        await page.waitForTimeout(400);
        await page.screenshot({ path: `${OUT}/panel-${id}-${vw}-${theme}.png` });
      });
    }
  }
  test("shot shell", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await gotoEditor(page);
    for (const tab of ["home", "insert", "layout", "references", "review", "view"]) {
      await page.locator(`[data-tab="${tab}"]`).click();
      await page.screenshot({ path: `${OUT}/shell-${tab}.png`, clip: { x: 0, y: 0, width: 1280, height: 160 } });
    }
    await page.screenshot({ path: `${OUT}/shell-statusbar.png`, clip: { x: 0, y: 760, width: 1280, height: 40 } });
    await page.screenshot({ path: `${OUT}/shell-full.png` });
  });
}
