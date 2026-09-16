import { readFile } from "node:fs/promises";

import {
  test,
  expect,
  gotoEditor,
  expectSaveEnabled,
  saveDocument,
  runAppMenuCommand,
} from "./fixtures.mjs";

const ODT = "org.oasis.opendocument.text";
const TEXT = "text.plain";

async function waitForOpenedDocument(page, name) {
  await expect(page.locator("#docTitle")).toHaveValue(name);
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, {
    timeout: 45_000,
  });
  await expectSaveEnabled(page);
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

  // `#saveFormat` is no longer a control the user operates — Save keeps the
  // source format, the way Word's Save does, and CHANGING format is File ▸
  // Export as <format>, which is Word's Save As. The select survives as the
  // state holder Save reads, so it is still the honest place to assert which
  // format a round trip landed on, and which exporters are registered.
  const format = page.locator("#saveFormat");
  await expect(format).toHaveValue(TEXT);
  await expect(format.locator("option")).toHaveText([
    "Normalized JSON",
    "ODT",
    "DOCX",
    "Plain text",
  ]);

  const downloadPromise = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.odt");
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

  const downloadPromise = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.odt");
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("opendoc-demo.odt");
  await expect(page.locator("#compatibilityStatus")).toBeVisible();
  await expect(page.locator("#compatibilityStatus")).toContainText(
    "export finding",
  );
  await expect(page.locator("#status")).toContainText("compatibility finding");
  expect(consoleErrors).toEqual([]);
});
