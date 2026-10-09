// File ▸ Export as Web Page shows the document as the page shows it (ADR-066).
//
// The owner: "HTML export fidelity is way too weak." It was: a document whose
// formatting lives in its STYLES — which is almost every real document — exported
// as the browser's defaults, because only direct formatting was read. The Title
// came out as plain body text, headings as black serif, every table as a grey
// grid with a bold first row.
//
// THE GUARD IS WHAT A READER SEES. The exported file is opened in a browser and
// the COMPUTED style of known elements is read, so an export that wrote the right
// CSS text in a place the browser ignores (or the browser's own `h1` size leaking
// through) cannot pass. The expected values are the sample's own styles: Title
// 30pt bold #102a43 with a rule under it, Heading 1 #2563eb, the suite table's
// header row #e8eef5, Calibri for body text, real list items.
import { test, expect, runAppMenuCommand } from "./fixtures.mjs";
import { readFile } from "node:fs/promises";

test("an exported web page shows the document's styles, tables and lists as the page does", async ({
  page,
  browser,
  consoleErrors,
}) => {
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("#file")).toBeEnabled();
  await page.locator("#file").setInputFiles("sample.docx");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });

  const download = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.html");
  const html = (await readFile(await (await download).path())).toString("utf8");

  const reader = await browser.newPage();
  await reader.setContent(html);
  const seen = await reader.evaluate(() => {
    const byText = (selector, text) =>
      [...document.querySelectorAll(selector)].find((element) => element.textContent.trim().startsWith(text));
    const style = (element) => (element ? getComputedStyle(element) : null);
    const title = byText("p,h1,h2,h3", "OpenDoc Feature Test Document");
    const heading = byText("h1", "Test suite overview");
    const headerCell = byText("th,td", "Feature group");
    const bullet = byText("li", "Typography and character formatting");
    const body = byText("p", "This document is intentionally varied");
    return {
      title: title && {
        size: style(title).fontSize,
        weight: style(title).fontWeight,
        color: style(title).color,
        rule: style(title).borderBottomStyle,
      },
      heading: heading && { tag: heading.tagName, color: style(heading).color, size: style(heading).fontSize },
      headerCell: headerCell && { tag: headerCell.tagName, fill: style(headerCell).backgroundColor },
      bullet: bullet && { inList: !!bullet.closest("ul"), marker: style(bullet).listStyleType },
      bodyFont: body && style(body).fontFamily,
    };
  });
  await reader.close();

  // 30pt is 40px.
  expect(seen.title).toEqual({ size: "40px", weight: "700", color: "rgb(16, 42, 67)", rule: "solid" });
  // A heading, in its style's blue at 16pt — not the browser's 2em black.
  expect(seen.heading).toEqual({ tag: "H1", color: "rgb(37, 99, 235)", size: "21.3333px" });
  // The header row the document marks as one, in its fill.
  expect(seen.headerCell).toEqual({ tag: "TH", fill: "rgb(232, 238, 245)" });
  // A real list item whose marker is the page's bullet.
  expect(seen.bullet.inList).toBe(true);
  expect(seen.bullet.marker).toContain("•");
  expect(seen.bodyFont).toMatch(/^Calibri, Carlito, sans-serif$/);
  expect(consoleErrors).toEqual([]);
});
