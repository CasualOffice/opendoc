// Who owns the status line: the reader, or the machine reporting on itself.
//
// The editor has two kinds of status message and they were going down one pipe.
//
//   * A READER's message — "Copied 11 characters", "Viewing mode is read-only;
//     switch to Editing to change the document". They asked for it or they have
//     to act on it, so it must survive until something the reader does replaces
//     it, and it goes to the live region and (where the footer is hidden) the
//     toast.
//   * BACKGROUND PROGRESS — "Ready — open a .docx…", "Loading the sample
//     document…", "Opening sample.docx…", "Rendering 14 pages at 100%…",
//     "Fetching web fonts for sample.docx…". Nobody asked. It must never be
//     announced, never toast, never reach an embedding host as editor status,
//     and above all never paint over the first kind.
//
// All five open-path lines are now `statusChannel.progress`, whose rule is in
// `status_policy.mjs` `backgroundProgressMayPaint`: it paints only onto an empty
// line or onto a line it wrote itself, and an empty text retires only its own.
//
// Both tests below are the consequences a reader experiences, and they are each
// other's precondition — which is the whole point of putting them in one file.
// Routing the render line through `progress` in isolation left "Opening
// sample.docx…" standing permanently, because that line was itself a reader
// message and was only ever cleared as a side effect of the render overwriting
// it. Measured: twelve seconds after the document was open and painted, the
// status bar still read "Opening opendoc-demo.docx…". Fixing one without the
// other converts a line that gets wiped into a line that never goes away.
import { test, expect, gotoEditor, clickIntoFirstPage, setReviewMode } from "./fixtures.mjs";

const statusText = (page) => page.locator("#status").textContent();

test("an open, painted document is not still reporting that it is opening", async ({
  page,
  consoleErrors,
}) => {
  // Deliberately NOT `gotoEditor`. Its own ready condition includes
  // `status.textContent === ""`, so a test that waits on it and then asserts the
  // same thing is a tautology — it can only run its assertion in the state it
  // already waited for. The first version of this guard was exactly that, and it
  // passed under a mutation that left "Opening sample.docx…" on screen forever
  // (the wait timed out instead, in the other test). So the readiness signals
  // here are the INDEPENDENT ones: pages painted, and the font upgrade done.
  await page.goto("/editor.html?fixture=rich");
  await page.waitForFunction(
    () =>
      document.querySelectorAll(".page-wrap").length > 0 &&
      document.body.dataset.fontsReady === "true",
    null,
    { timeout: 45_000 },
  );

  // Not a snapshot of one string: ANY of the five open-path lines standing here
  // is the defect, and naming them individually is how the sixth one escapes.
  const text = (await statusText(page)) ?? "";
  expect(
    text,
    `the status line still reads ${JSON.stringify(text)} over an open, painted document`,
  ).toBe("");

  expect(consoleErrors).toEqual([]);
});

test("a re-render does not wipe the refusal the reader just earned", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await setReviewMode(page, "viewing");
  await clickIntoFirstPage(page);

  // Earn a reader's message, and specifically a REFUSAL — the kind that names a
  // reason and a way out, so losing it leaves the reader with a keystroke that
  // did nothing and no explanation. A refused keystroke is used rather than a
  // copy confirmation because the clipboard needs a permission the browser does
  // not grant a test run, and a guard that depends on one is a guard that
  // reports the wrong thing when it is withheld.
  await page.keyboard.type("X");
  await expect(page.locator("#status")).toContainText("read-only");
  const earned = await statusText(page);

  // Now force a full re-render, which is what a zoom, a reflow and the font
  // upgrade each do. Through `setStatus` the render announced itself over the
  // top of the confirmation and then cleared the line, so the reader's answer
  // appeared and vanished within one interaction.
  await page.locator("#zoomIn").click();
  await expect(page.locator("#zoom")).not.toHaveValue("100%");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0);

  await expect(
    page.locator("#status"),
    "the re-render took the status line away from the reader",
  ).toHaveText(earned);

  expect(consoleErrors).toEqual([]);
});
