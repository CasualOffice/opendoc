import { readFile } from "node:fs/promises";

import { test, expect, gotoEditor } from "./fixtures.mjs";

const ODT = "org.oasis.opendocument.text";
const TEXT = "text.plain";

async function waitForOpenedDocument(page, name) {
  await expect(page.locator("#docTitle")).toHaveValue(name);
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, {
    timeout: 45_000,
  });
  await expect(page.locator("#save")).toBeEnabled();
}

test("browser Open and Save dispatch text through the generic ODT exporter", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("#file")).toBeEnabled();
  await page.locator("#file").setInputFiles({
    name: "notes.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("Alpha\nBeta\n", "utf8"),
  });
  await waitForOpenedDocument(page, "notes.txt");

  const format = page.locator("#saveFormat");
  await expect(format).toHaveValue(TEXT);
  await expect(format.locator("option")).toHaveText([
    "Normalized JSON",
    "ODT",
    "DOCX",
    "Plain text",
  ]);
  await format.selectOption(ODT);

  const downloadPromise = page.waitForEvent("download");
  await page.locator("#save").click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("notes.odt");
  const path = await download.path();
  const bytes = await readFile(path);
  expect(bytes.subarray(0, 2).toString()).toBe("PK");

  await page.locator("#file").setInputFiles({
    name: "roundtrip.odt",
    mimeType: "application/vnd.oasis.opendocument.text",
    buffer: bytes,
  });
  await waitForOpenedDocument(page, "roundtrip.odt");
  await expect(format).toHaveValue(ODT);
  expect(consoleErrors).toEqual([]);
});

test("cross-format browser Save visibly reports compatibility findings", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.locator("#saveFormat").selectOption(ODT);

  const downloadPromise = page.waitForEvent("download");
  await page.locator("#save").click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("opendoc-demo.odt");
  await expect(page.locator("#compatibilityStatus")).toBeVisible();
  await expect(page.locator("#compatibilityStatus")).toContainText(
    "export finding",
  );
  await expect(page.locator("#status")).toContainText("compatibility finding");
  expect(consoleErrors).toEqual([]);
});
