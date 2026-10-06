// Word's built-in styles are shown by Word's UI names, and applied by their
// stored names.
//
// `?fixture=styled` stores Word's Heading 1 the way Word itself and LibreOffice
// write it — `w:name="heading 1"` — so it is the document that showed the
// defect: the Styles box, its search and the palette all offered "heading 1"
// (`desk-08c`, `desk-08f`). Word shows "Heading 1". What must NOT change is the
// value: the style is still found, applied and reported by its stored name,
// because that is what the engine looks up and what export writes back.
import { test, expect, MOD } from "./fixtures.mjs";

async function gotoStyled(page) {
  await page.goto("/editor.html?fixture=styled");
  await page.waitForFunction(() => !document.getElementById("stylesTrigger")?.disabled, null, {
    timeout: 45_000,
  });
}

test("a built-in style reads as Word names it, everywhere a reader meets it", async ({
  page,
  consoleErrors,
}) => {
  await gotoStyled(page);

  // The Styles box, searched by the name a reader sees.
  await page.locator("#stylesTrigger").click();
  await page.locator("#stylesMenuInput").fill("Heading");
  const option = page.locator('#stylesMenu .style-option[data-style="heading 1"]');
  await expect(option).toBeVisible();
  await expect(option.locator(".style-option-name")).toHaveText("Heading 1");

  // Applied by its STORED name: the control reports the identity, and shows the label.
  await option.click();
  await expect(page.locator("#stylesTrigger")).toHaveAttribute("data-active-style", "heading 1");
  await expect(page.locator("#stylesTriggerLabel")).toHaveText("Heading 1");

  // The palette row, which keeps the stored name in its command id.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("heading 1");
  const row = page.locator('#cmdList .cmd-item[data-command-id="style.heading 1"]');
  await expect(row).toBeVisible();
  await expect(row).toContainText("Style: Heading 1");
  await expect(row).not.toContainText("heading 1");
  await page.keyboard.press("Escape");

  // Paragraph properties' full Style list: the value stays the stored name.
  const listed = await page
    .locator('#paraPanelStyle option[value="heading 1"]')
    .evaluate((node) => ({ value: node.value, text: node.textContent }));
  expect(listed).toEqual({ value: "heading 1", text: "Heading 1" });
  expect(consoleErrors).toEqual([]);
});
