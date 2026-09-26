// What the IMPORT lost has to be visible, not just computed.
//
// `importReportJson` has been a shipped engine getter with ZERO consumers in
// `webapp/` — loss was calculated on every open and thrown away, while
// export-side loss was reported. The open path instead called
// `showCompatibilityFindings(0, "export")`, which actively cleared the
// indicator, so the editor said nothing about a document it had just failed to
// represent completely.
//
// SKILL §1 lists "direct OOXML with verbatim retention" as one of three
// structural advantages over ONLYOFFICE — and states the condition: "this
// advantage is only real if loss is detected AND reported". Detection was
// already there. This is the reporting half.
//
// It is also structurally impossible for ONLYOFFICE to match: their pipeline is
// DOCX → Editor.bin → DOCX, so whatever the intermediate model lacks is gone
// before anything could report it.
import { test, expect } from "./fixtures.mjs";

async function openFixture(page, fixture) {
  await page.goto(`/editor.html?fixture=${fixture}`);
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
}

const indicator = (page) => page.locator("#compatibilityStatus");

test("opening a document reports what the import could not represent", async ({
  page,
  consoleErrors,
}) => {
  await openFixture(page, "demo");

  // The fixture is a deliberately hostile compatibility document, so it has
  // findings. The assertion is on the SHAPE, not on a pinned count: a count
  // would break every time the importer improves, which is the wrong incentive
  // — improving import should never fail this test.
  await expect(indicator(page)).toBeVisible();
  await expect(indicator(page)).toHaveText(/^\d[\d,]* import findings?$/);
  await expect(indicator(page)).toHaveAttribute(
    "title",
    /compatibility findings? reported during import/,
  );

  const count = Number((await indicator(page).textContent()).replace(/[^\d]/g, ""));
  expect(count, "the compatibility fixture must report some import loss").toBeGreaterThan(0);

  expect(consoleErrors).toEqual([]);
});

test("the indicator says IMPORT on open, not export", async ({ page, consoleErrors }) => {
  // The bug this replaces was not "nothing was shown" — it was that the open
  // path reported a cleared EXPORT count, so the one phase the user had
  // actually performed was the one phase never described.
  await openFixture(page, "demo");
  await expect(indicator(page)).toContainText("import");
  await expect(indicator(page)).not.toContainText("export");
  expect(consoleErrors).toEqual([]);
});

test("a document the importer handles cleanly says nothing at all", async ({
  page,
  consoleErrors,
}) => {
  // The indicator must stay quiet when there is nothing to say, or it becomes
  // chrome people learn to ignore. A blank document was never imported, so the
  // engine hands back no report at all — `importFindingCount` treats that as
  // zero rather than throwing, which is what stops a missing report from
  // breaking document open.
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(
    () => document.getElementById("compatibilityStatus") !== null,
    null,
    { timeout: 45_000 },
  );
  await expect(indicator(page)).toBeHidden();
  expect(consoleErrors).toEqual([]);
});
