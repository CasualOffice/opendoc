// ADR-061 behind ADR-065: "Keep as tracked changes" writes a comparison INTO the
// document.
//
// A comparison is first SHOWN, read-only, as a redline on the page
// (`compare.spec.mjs` covers that). Keeping it is the reader's explicit second
// step, and this file covers what that step must do: put real tracked changes
// into the reader's document, refuse — in its own words — a document that
// already has some, and report what tracked changes cannot express.
//
// SO THE ASSERTIONS ARE ABOUT WHAT THE CANVAS PAINTS. `apply_revision_markup`
// (`casual-doc-layout/src/flow.rs:168`) stamps an author hue from a ten-colour
// palette onto the runs of a revision — underline for an insertion, strikethrough
// for a deletion — and every one of those ten hues is strongly chromatic
// (`REVIEW_AUTHOR_HUES`, byte-identical to the webapp overlay's). Black text and
// its antialiasing are not: r, g and b move together. So "is there saturated ink
// on page one" is a question about the rendered page that a plain document answers
// no to and a document carrying a comparison answers yes to, and it cannot be
// satisfied by the call merely having happened.
//
// The precondition is asserted rather than assumed (SKILL: create the condition,
// keep the precondition explicit). A guard that only checked "saturated ink > 0"
// after the fact would pass on a document that was already coloured.
//
// `keyboard.insertText`, never `keyboard.type`: printable characters are
// `preventDefault`ed on this editor, so `type` leaves the element with no input
// and the spec green over a feature that never ran.
import {
  test,
  expect,
  clickIntoFirstPage,
  mirrorBlocks,
  moveCaretToDocStart,
  setReviewMode,
} from "./fixtures.mjs";

/** A plain-text fixture, for the reason `compare.spec.mjs` gives: a comparison
 *  imports both sides through the format registry, so `.txt` exercises the same
 *  path a `.docx` would, and every difference it reports can be accounted for by
 *  reading three lines of this file. */
function textFile(name, body) {
  return { name, mimeType: "text/plain", buffer: Buffer.from(body, "utf8") };
}

async function openText(page, file, { lang = "" } = {}) {
  await page.goto(`/editor.html?blank=1${lang ? `&lang=${lang}` : ""}`);
  await expect(page.locator("#file")).toBeEnabled();
  await page.locator("#file").setInputFiles(file);
  await expect(page.locator("#docTitle")).toHaveValue(file.name);
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
}

/** Saturated (author-coloured) pixels on page one's rendered canvas.
 *
 *  `document.fonts.ready` first, for the reason `line-numbers.spec.mjs` records:
 *  a fallback face paints first and a measurement taken before the real one lands
 *  is a coin toss rather than a number.
 *
 *  The threshold is on CHROMA — max channel minus min channel — not on a specific
 *  hue. Pinning the expected RGB would pin the test to whichever of the ten
 *  palette entries the author name happens to hash to, and the author name here is
 *  a file name; a guard that reddens because a hue moved would be the
 *  "asserts the circumstance, not the guarantee" shape. The guarantee is that the
 *  review markup paints in an author colour at all. */
async function saturatedInk(page) {
  await page.evaluate(() => document.fonts.ready);
  return page.evaluate(() => {
    const canvas = document
      .querySelector('#pages .page-wrap[data-page-number="1"]')
      .querySelector("canvas.page");
    const { data } = canvas
      .getContext("2d")
      .getImageData(0, 0, canvas.width, canvas.height);
    let saturated = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] < 128) continue;
      const max = Math.max(data[i], data[i + 1], data[i + 2]);
      const min = Math.min(data[i], data[i + 1], data[i + 2]);
      // 48 of 255 is well below every palette hue's chroma (the least saturated,
      // `#12805c`, spans 0x6e) and well above what antialiased black text and the
      // page's own greys produce.
      if (max - min > 48) saturated += 1;
    }
    return saturated;
  });
}

/** Opens Compare from the rail, runs a comparison against `other`, waits for
 *  the redline on the page, and KEEPS it — the explicit step that writes it into
 *  the document. Waits on state changes rather than on a clock. */
async function compareAndKeep(page, other) {
  await page.locator("#railCompare").click();
  await expect(page.locator("#comparePanel")).toBeVisible();
  await expect(page.locator('#compareBody [data-compare-action="choose-file"]')).toBeEnabled();
  await page.locator("#compareFile").setInputFiles(other);
  await expect(page.locator("#compareBody [data-compare-view]")).toBeVisible({ timeout: 45_000 });
  await page.locator('#compareBody [data-compare-action="keep"]').click();
}

test("Keep writes one text edit into the document as a tracked change", async ({ page, consoleErrors }) => {
  // One word replaced, which is the fixture the owner's report was about: a
  // `text` difference, the kind that has somewhere in this document to be
  // marked. ("Delta" is the other document's word and "gamma" is ours, so the
  // comparison owes one insertion and one deletion in one paragraph — the
  // engine's `RevisionGroupKind::Replacement`.)
  await openText(page, textFile("mine.txt", "Alpha beta gamma\n"));

  // THE PRECONDITION, asserted. A black-text document paints no saturated ink,
  // and if that were ever false the assertion below would prove nothing.
  const before = await saturatedInk(page);
  expect(before, "a plain black-text document must paint no author-coloured ink").toBe(0);
  await expect(page.locator("#reviewSidebar .review-margin-revision")).toHaveCount(0);

  await compareAndKeep(page, textFile("theirs.txt", "Alpha beta delta\n"));
  await expect(page.locator("#compareBody [data-compare-marked]")).toBeVisible({ timeout: 45_000 });

  // THE MILESTONE. Author-coloured ink is now on the page, which means
  // `apply_revision_markup` ran over a revision this comparison wrote — the
  // underline on the insertion, the strikethrough on the deletion, both in the
  // author hue. Waited for rather than sampled once: the comparison finishes,
  // then the page repaints.
  await expect(async () => {
    expect(await saturatedInk(page)).toBeGreaterThan(0);
  }).toPass({ timeout: 45_000 });

  // And the markup view is ON, which is what makes it visible without the reader
  // finding a toggle. ADR-061: "turn `setShowChanges` on when a comparison
  // produces changes".
  await expect(page.locator("body")).toHaveClass(/showing-changes/);

  // THE REVIEW SURFACE READS IT, which is the half a pixel count cannot tell you
  // and the half that makes the change acceptable and rejectable. One card, and
  // specifically a REPLACEMENT card — the engine writes the recorded deletion and
  // the insertion as one `RevisionGroupKind::Replacement` so the pair decides
  // together — authored to the compared document and naming both words.
  //
  // Measured in Chromium through this exact route: the card reads
  // `Replaced “delta” with “gamma”`, authored `theirs.txt`. "delta" is the other
  // document's word, so a card carrying it proves the comparison's DELETION
  // reached the model and not merely the panel.
  //
  // NOT the accessibility mirror, which was the first draft and was wrong about
  // the engine: `collect_a11y_inlines` drops a deletion's text on purpose ("a
  // deletion is not on the page under the default projection, so it is not
  // read"), so asserting "delta" in the mirror asserted against a deliberate
  // decision rather than against this change.
  const card = page.locator("#reviewSidebar .review-margin-revision");
  await expect(card).toHaveCount(1);
  await expect(card).toHaveClass(/review-margin-replacement/);
  await expect(card).toContainText("delta");
  await expect(card).toContainText("gamma");
  // ATTRIBUTED TO THE COMPARED DOCUMENT (ADR-061), so the author colour on the
  // canvas distinguishes a computed difference from a person's suggestion.
  await expect(card).toContainText("theirs.txt");

  // And `updateReviewControls` disables Accept all on a document with no tracked
  // changes, so an enabled one is the bulk-decide path agreeing too.
  await expect(page.locator("#reviewAcceptAll")).toBeEnabled();

  // The document's own word survived: a comparison marks text, it does not
  // replace it.
  expect((await mirrorBlocks(page)).join(" ")).toContain("gamma");

  // The panel is the INDEX of that, and it leads with a sentence about the
  // document rather than with a bare count.
  await expect(page.locator("#compareBody [data-compare-marked]")).toBeVisible();
  await expect(page.locator("#compareBody")).toContainText(/tracked changes in this document/i);
  expect(consoleErrors).toEqual([]);
});

test("Keep refuses a document that already carries revisions, in its own words", async ({
  page,
  consoleErrors,
}) => {
  // ADR-061 refuses rather than accepting the reviewer's suggestions first, which
  // is what ONLYOFFICE does on consent (`Comparison.js:3910-3921`). Destroying
  // someone's suggestions to run a comparison is the loss `AGENTS.md` puts first.
  await openText(page, textFile("mine.txt", "Alpha beta gamma\n"));

  // A REAL tracked change, authored the way a reviewer authors one.
  await setReviewMode(page, "suggesting");
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.insertText("Suggested. ");
  await expect(page.locator("#reviewAcceptAll")).toBeEnabled();
  const bodyBefore = await mirrorBlocks(page);

  // VIEWING the comparison is not refused (ADR-065): nothing is decided on a
  // redline, so the reviewer's suggestion and the comparison's marks cannot
  // decide each other. Only Keep writes, and Keep is what refuses.
  await compareAndKeep(page, textFile("theirs.txt", "Alpha beta delta\n"));

  // ITS OWN SENTENCE, routed by the engine's `compare.document-has-revisions`
  // code — and it says what we refuse and why rather than apologising.
  const refusal = page.locator("#compareBody [data-compare-refused]");
  await expect(refusal).toBeVisible({ timeout: 45_000 });
  await expect(refusal).toHaveAttribute("data-compare-refused", "compare.document-has-revisions");
  await expect(refusal).toContainText(/already has tracked changes/i);
  await expect(refusal).toContainText(/accept or reject them first/i);
  await expect(refusal).not.toContainText(/sorry|not supported|not yet/i);

  // AND THE REVIEWER'S SUGGESTION SURVIVED, which is the whole reason for the
  // refusal. A refusal that still mutated the document would be worse than no
  // refusal at all.
  expect(await mirrorBlocks(page)).toEqual(bodyBefore);
  expect(consoleErrors).toEqual([]);
});

test("a difference a tracked change cannot express is REPORTED, not swallowed", async ({
  page,
  consoleErrors,
}) => {
  // A whole paragraph the other document has and this one does not. There is
  // nowhere in this document to mark it — `UpdateReviewState` edits the inlines of
  // paragraphs that EXIST and cannot add one — so the engine reports
  // `blockDeletion` on `EditResult.pasteLoss` rather than approximating it by
  // marking a neighbour.
  //
  // A comparison that applied some of its changes and reported success is the
  // worst outcome available, so this is the guard that the report reaches a
  // reader. It also has to be a comparison that DID apply something, or the
  // report would be the only thing on screen and could not be mistaken for
  // silent: the extra sentence in "mine" gives it one insertion to land.
  await openText(page, textFile("mine.txt", "Alpha\nBeta\nGamma only in mine\n"));
  await compareAndKeep(
    page,
    textFile("theirs.txt", "Alpha\nBeta\nDelta only in theirs\nEpsilon only in theirs\n"),
  );

  const report = page.locator('#compareBody [data-compare-unmarked="blockDeletion"]');
  await expect(report).toBeVisible({ timeout: 45_000 });
  // A SENTENCE, not the engine's key. A loss report dropped — or printed as a
  // dotted identifier — because the host has no wording for it is the defect
  // twice over.
  await expect(report).toContainText(/paragraphs/i);
  await expect(report).not.toContainText("blockDeletion");
  await expect(page.locator("#compareBody")).toContainText(/not marked in the document/i);
  // NON-VACUITY: the comparison still applied what it could, so the report is a
  // qualification of a result rather than the whole result.
  await expect(page.locator("#compareBody [data-compare-marked]")).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("the refusal is the READER's language, not the engine's English", async ({
  page,
  consoleErrors,
}) => {
  // THIS IS THE GUARD ON THE WIRING, and it exists because the refusal guard
  // above cannot be it. The engine's own sentence for
  // `compare.document-has-revisions` and the catalogue's English sentence say the
  // same thing — deliberately, they describe one decision — so an assertion on
  // English prose passes whether the code was routed through
  // `editRefusalMessage`'s `routeRefusal` seam or fell through to the engine's
  // `refused: ` text. That is the vacuous shape this repository has shipped
  // before, and it would have hidden the routing being absent entirely.
  //
  // A non-English locale separates them. `casual_doc_edit::refusal`'s whole
  // purpose is that "a host routes the code through its own catalogue — `t(code)`
  // — so a non-English reader gets the specific reason in their own language",
  // and nothing in the chrome read a `compare.*` code before this lane. German
  // here: routed, the panel is German; unrouted, it is the engine's English, and
  // the last two assertions cannot both hold.
  await openText(page, textFile("mine.txt", "Alpha beta gamma\n"), { lang: "de" });
  await setReviewMode(page, "suggesting");
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.insertText("Vorgeschlagen. ");
  await expect(page.locator("#reviewAcceptAll")).toBeEnabled();

  await compareAndKeep(page, textFile("theirs.txt", "Alpha beta delta\n"));

  const refusal = page.locator("#compareBody [data-compare-refused]");
  await expect(refusal).toBeVisible({ timeout: 45_000 });
  await expect(refusal).toHaveAttribute("data-compare-refused", "compare.document-has-revisions");
  // The German catalogue's sentence, which only the routed path can produce.
  await expect(refusal).toContainText(/nachverfolgte Änderungen/);
  await expect(refusal).toContainText(/Nehmen Sie sie zuerst an oder lehnen Sie sie ab/);
  // And NOT the engine's English, which is what an unrouted code would show.
  await expect(refusal).not.toContainText(/already has tracked changes/i);
  expect(consoleErrors).toEqual([]);
});
