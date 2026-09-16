// docs/108 phase 1 — paragraph-level tracked changes that arrive in a Word file.
//
// A document whose reviewer split paragraphs, joined them, or recentred one used to
// open looking clean: the changes were kept on save but never listed, painted,
// navigable or decidable, and Accept All reported "no tracked revisions"
// (docs/104 HF-156). This drives the whole reviewer path through the real UI.
import { test, expect, gotoEditor } from "./fixtures.mjs";
import { makeParagraphRevisionsDocx } from "./large-docx.mjs";

async function openParagraphRevisions(page) {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles({
    name: "paragraph-revisions.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: Buffer.from(makeParagraphRevisionsDocx()),
  });
  await expect(page.locator("#docTitle")).toHaveValue("paragraph-revisions.docx");
  return page.locator("#reviewSidebar");
}

test("paragraph-level suggestions from a Word file are listed, drawn and described", async ({
  page,
  consoleErrors,
}) => {
  const sidebar = await openParagraphRevisions(page);
  const deletion = sidebar.locator(".review-margin-card.review-margin-paragraph-mark-deletion");
  const format = sidebar.locator(".review-margin-card.review-margin-paragraph-format");
  const insertion = sidebar.locator(".review-margin-card.review-margin-paragraph-mark-insertion");

  await expect(deletion).toHaveCount(1);
  await expect(format).toHaveCount(1);
  await expect(insertion).toHaveCount(1);
  // Say what the suggestion is, in words a reviewer uses — including what
  // accepting a deleted break will do, which "deleted" alone does not convey.
  await expect(deletion).toContainText("Deleted a paragraph break");
  await expect(deletion).toContainText("joins this paragraph to the next");
  await expect(format).toContainText("Formatted paragraph");
  await expect(format).toContainText("Alignment: inherited → Centered");
  await expect(insertion).toContainText("Added a paragraph break");

  // Drawn where they are: a pilcrow at each changed mark, a bar beside the
  // reformatted paragraph.
  await expect(page.locator(".review-paragraph-mark-marker")).toHaveCount(2);
  await expect(page.locator(".review-paragraph-mark-deleted")).toHaveCount(1);
  await expect(page.locator(".review-paragraph-mark-inserted")).toHaveCount(1);
  await expect(page.locator(".review-paragraph-format-bar")).toHaveCount(1);

  // Nothing is selected yet, so nothing may look selected. Every ungrouped
  // marker used to render active on open, because "no active item" (null)
  // matched the null slots in each marker's own id list.
  await expect(page.locator(".review-revision-marker-active")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("accepting a deleted paragraph break joins the paragraphs, and undo splits them back", async ({
  page,
  consoleErrors,
}) => {
  const sidebar = await openParagraphRevisions(page);
  const cards = sidebar.locator(".review-margin-card");
  const deletion = sidebar.locator(".review-margin-card.review-margin-paragraph-mark-deletion");
  await expect(cards).toHaveCount(3);

  await deletion.click();
  await deletion.getByRole("button", { name: "Accept" }).click();
  await expect(deletion).toHaveCount(0);
  await expect(cards).toHaveCount(2, { timeout: 10_000 });
  // The join really happened: two paragraphs became one. The accessibility
  // mirror is projected from the model, one <p> per paragraph.
  const paragraphs = page.locator("#a11yDocument p");
  await expect(paragraphs).toHaveCount(3);
  await expect(paragraphs.first()).toHaveText("AlphaBeta");
  await expect(page.locator(".review-paragraph-mark-marker")).toHaveCount(1);

  await page.locator("#undoBtn").click();
  await expect(cards).toHaveCount(3);
  await expect(paragraphs).toHaveCount(4);
  await expect(paragraphs.first()).toHaveText("Alpha");
  await expect(page.locator(".review-paragraph-mark-marker")).toHaveCount(2);

  expect(consoleErrors).toEqual([]);
});

test("Next reaches paragraph-level suggestions, and Accept all decides them", async ({
  page,
  consoleErrors,
}) => {
  const sidebar = await openParagraphRevisions(page);
  const cards = sidebar.locator(".review-margin-card");
  await expect(cards).toHaveCount(3);

  // A paragraph mark carries no text, and the navigation used to skip every
  // text-less change except run formatting, so Next stepped straight past them.
  // Each step is asserted in document order, and waits for the card to change.
  const expanded = sidebar.locator('.review-margin-card[aria-expanded="true"]');
  for (const kind of ["paragraph-mark-deletion", "paragraph-format", "paragraph-mark-insertion"]) {
    await page.locator("#reviewNext").click();
    await expect(expanded).toHaveCount(1);
    await expect(expanded).toHaveClass(new RegExp(`review-margin-${kind}`));
  }

  // The formatting bar sits in the page margin, left of the text column, not
  // beside the centred text it describes.
  const bar = page.locator(".review-paragraph-format-bar");
  const pageBox = await page.locator(".page-wrap .page").first().boundingBox();
  const barBox = await bar.boundingBox();
  expect(barBox.x - pageBox.x).toBeLessThan(pageBox.width * 0.2);

  await page.locator("#reviewAcceptAll").click();
  await expect(cards).toHaveCount(0);
  await expect(page.locator(".review-paragraph-mark-marker")).toHaveCount(0);
  await expect(page.locator(".review-paragraph-format-bar")).toHaveCount(0);
  // Word's Accept All: Alpha's deleted break joins it to Beta; Gamma's inserted
  // break is kept, so Gamma and Delta stay separate.
  await expect(page.locator("#a11yDocument p")).toHaveText(["AlphaBeta", "Gamma", "Delta"]);

  expect(consoleErrors).toEqual([]);
});
