// Compare with another document: reachable, and it really finds the differences.
//
// `docs/153` `review.compare-documents`, rank 2, and the third of the three
// capabilities on this branch that were completely built and completely
// unreachable. `crates/casual-doc-diff` is a whole crate and
// `crates/casual-doc-wasm/src/diff.rs` exposes six functions over it; `grep -r
// diffVersions webapp/` returned nothing, and the version panel disabled Show
// changes with a comment saying the structural diff "is not built".
//
// THE GUARD IS THE EFFECT. A spec that found a Compare button and a panel would
// pass over a chrome that compared the document with itself, or compared the two
// sides the wrong way round — which is the mistake a reader cannot detect,
// because every addition would be reported as a removal and the count would be
// right. So the fixtures are built here, by hand, with KNOWN differences and in a
// known direction, and the assertions are about which differences come back.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  runAppMenuCommand,
  saveDocument,
} from "./fixtures.mjs";

const MOD = process.platform === "darwin" ? "Meta" : "Control";

/** A plain-text document, which the engine imports and the comparison parses.
 *
 *  Text rather than DOCX on purpose: a comparison imports both sides through the
 *  format registry, so a `.txt` exercises the same path a `.docx` would, and a
 *  fixture whose exact content is three lines in this file is a fixture whose
 *  every reported difference can be accounted for. A binary fixture would make
 *  "one insertion" an assertion about a file nobody in this test can read. */
function textFile(name, body) {
  return { name, mimeType: "text/plain", buffer: Buffer.from(body, "utf8") };
}

async function openText(page, file) {
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("#file")).toBeEnabled();
  await page.locator("#file").setInputFiles(file);
  await expect(page.locator("#docTitle")).toHaveValue(file.name);
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
}

/** Opens the Compare panel through one of its two surfaces and runs a comparison
 *  against `other`, waiting for the result rather than for a timeout. */
async function compareAgainst(page, other, { via }) {
  if (via === "rail") {
    await page.locator("#railCompare").click();
  } else {
    await page.locator('[data-tab="review"]').click();
    await page.locator("#reviewCompareBtn").click();
  }
  await expect(page.locator("#comparePanel")).toBeVisible();
  // The chooser button is the affordance and is asserted; the FILES go on the
  // input, which is how every open-a-file spec in this suite works — driving the
  // button opens a native file chooser Playwright then has to hold open for the
  // rest of the test.
  await expect(page.locator('#compareBody [data-compare-action="choose-file"]')).toBeEnabled();
  await page.locator("#compareFile").setInputFiles(other);
  // Waits on a STATE CHANGE, not a clock: the total appears when the comparison
  // finishes. A `waitForTimeout` here would be the clock-bound shape that
  // degrades under load however sound the code is.
  await expect(page.locator("#compareBody [data-compare-total]")).toBeVisible({
    timeout: 45_000,
  });
}

test("Compare is reachable from the Review band and from the rail, and both open one panel", async ({
  page,
}) => {
  await gotoEditor(page);
  // The rail entry.
  await expect(page.locator("#railCompare")).toBeEnabled();
  await page.locator("#railCompare").click();
  await expect(page.locator("#comparePanel")).toBeVisible();
  await expect(page.locator("#railCompare")).toHaveAttribute("aria-pressed", "true");
  // It says what it DOES before anything is picked, and ADR-061 changed what that
  // is. Word and Google Docs build a merged third document; we write the
  // differences into the document on screen as tracked changes, which is
  // ONLYOFFICE's answer — and that is the warning that matters, because the
  // gesture mutates the open document. The old sentence promised the opposite
  // ("they cannot be accepted or rejected") and is asserted gone, so the stale
  // claim cannot come back.
  await expect(page.locator("#compareBody")).toContainText(/as tracked changes/i);
  await expect(page.locator("#compareBody")).not.toContainText(
    /cannot be accepted or rejected/i,
  );
  await page.locator("#compareClose").click();
  await expect(page.locator("#comparePanel")).toBeHidden();

  // The Review band's face, which is the SAME command and therefore the same
  // panel — not a second surface with a second implementation behind it.
  await page.locator('[data-tab="review"]').click();
  await expect(page.locator("#reviewCompareBtn")).toBeEnabled();
  await page.locator("#reviewCompareBtn").click();
  await expect(page.locator("#comparePanel")).toBeVisible();

  // And the Review MENU, which is what matters in the compact chrome: the ribbon
  // is hidden there, so a command with only a band face would be palette-only.
  await runAppMenuCommand(page, "review", "review.compare");
  await expect(page.locator("#comparePanel")).toBeVisible();
});

test("a paragraph this document has and the other lacks is reported as ADDED", async ({
  page,
  consoleErrors,
}) => {
  // THE DIRECTION IS THE POINT, and it is the half a count cannot check. The
  // other document is the left (older) side and this one is the right, which is
  // review's own orientation — so a paragraph only this document has is an
  // ADDITION. A comparison wired the other way round would report the same
  // NUMBER of differences and call every addition a removal, which is a mistake
  // no reader could detect from the output.
  //
  // One whole extra BLOCK, so there is exactly one difference and nothing to
  // argue about. (An edited line is a different and weaker fixture: the engine
  // diffs it inside the block and reports the changed WORD, so "Only in mine"
  // against "Only in theirs" is one `text` change reading "mine" — true, useful,
  // and no use at all for proving a direction.)
  await openText(page, textFile("mine.txt", "Alpha\nBeta\nGamma\nDelta only in mine\n"));
  await compareAgainst(page, textFile("theirs.txt", "Alpha\nBeta\nGamma\n"), { via: "rail" });

  await expect(page.locator("#compareBody")).toContainText("theirs.txt");
  await expect(page.locator("#compareBody [data-compare-total]")).toHaveAttribute(
    "data-compare-total",
    "1",
  );
  const added = page.locator('#compareBody [data-compare-kind="insertion"]');
  await expect(added).toHaveCount(1);
  await expect(added).toContainText("Delta only in mine");
  // Not a deletion, which is what makes the line above mean something.
  await expect(page.locator('#compareBody [data-compare-kind="deletion"]')).toHaveCount(0);
  // Grouped and counted by the engine's own construct family — a whole paragraph
  // appearing is `block`, not `text`.
  await expect(page.locator('#compareBody [data-compare-family="block"]')).toHaveCount(1);
  expect(consoleErrors).toEqual([]);
});

test("a paragraph the other document has and this one lacks is reported as REMOVED", async ({
  page,
  consoleErrors,
}) => {
  // The same fixture inverted. Both halves are needed: a comparison that reported
  // EVERYTHING as an insertion would pass the test above on its own.
  await openText(page, textFile("mine.txt", "Alpha\nBeta\n"));
  await compareAgainst(page, textFile("theirs.txt", "Alpha\nBeta\nGamma only in theirs\n"), {
    via: "review-band",
  });

  await expect(page.locator("#compareBody [data-compare-total]")).toHaveAttribute(
    "data-compare-total",
    "1",
  );
  const removed = page.locator('#compareBody [data-compare-kind="deletion"]');
  await expect(removed).toHaveCount(1);
  await expect(removed).toContainText("Gamma only in theirs");
  await expect(page.locator('#compareBody [data-compare-kind="insertion"]')).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("comparing a document with itself reports no differences", async ({ page, consoleErrors }) => {
  // The control, and it is not redundant: without it, a chrome that reported
  // every block as changed would satisfy the "finds differences" test above by
  // finding far too many, and nothing would notice. "No differences" is the
  // hardest answer for a broken comparison to produce.
  const body = "One\nTwo\nThree\n";
  await openText(page, textFile("same.txt", body));
  await page.locator("#railCompare").click();
  await expect(page.locator("#comparePanel")).toBeVisible();
  await page.locator("#compareFile").setInputFiles(textFile("same-copy.txt", body));
  await expect(page.locator("#compareBody")).toContainText(/No differences/i, {
    timeout: 45_000,
  });
  await expect(page.locator("#compareBody [data-compare-total]")).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("Show changes on a version compares it with its PREDECESSOR, and writes nothing", async ({
  page,
  consoleErrors,
}) => {
  // THIS TEST REPLACES ONE THAT COULD NOT FAIL, and the replacement is the point.
  //
  // The assertion that stood here was
  //
  //     await expect(page.locator("#compareBody"))
  //       .toContainText(/Compared with|No differences/i);
  //
  // which passes whether the comparison found every difference or none —
  // `docs/164` §8 row 12 and `SKILL` §4: a guard that cannot fail is worse than
  // no guard, because it gets cited as evidence. It was cited as evidence, and
  // the feature under it had never worked from the route a reader takes.
  //
  // IT ALSO TOOK THE WRONG ROUTE. It opened the ⋮ without ever clicking the row,
  // and `entry` and `actionCell` are SIBLINGS in `version_panel.mjs`
  // (`item.append(entry, actionCell)`), so a press on the ⋮ does not reach the
  // row's own click handler and no preview opens. Every reader opens the preview
  // first — clicking a row is how you look at a version — and that click runs
  // `void openPreview(row.versionId)`, after which `main.js` does `doc =
  // previewDoc`. The panel's own right-hand side is `comparableBytes(doc, …)`,
  // a freshly parsed preview has `revision == 0`, and `ExportMode::ExactIfUnchanged`
  // then returns the retained ORIGINAL BYTES VERBATIM. So "mine" was byte-identical
  // to the checkpoint handed in as "theirs" and the comparison could not find
  // anything. The old spec's route is the one route in which the bug is invisible.
  //
  // So this test clicks the row, which is what makes it able to fail.
  await gotoEditor(page);
  // THREE rows, because the rule under test is about a PREDECESSOR and a rule
  // stated over two rows cannot distinguish "its predecessor" from "the oldest
  // one". The import baseline, then two saves that each really changed the
  // document — a Save over an unmodified document reports `history.unchanged`
  // and writes nothing (#782's content identity), so the typing is not padding.
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.type("Alpha only in the middle version. ");
  await saveDocument(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.type("Beta only in the newest version. ");
  await saveDocument(page);
  await runAppMenuCommand(page, "file", "file.versionHistory");
  await expect(page.locator("#versionPanel")).toBeVisible();

  const rows = page.locator("#versionPanelBody .version-item");
  await expect(rows, "three versions, or the predecessor rule is untestable").toHaveCount(3, {
    timeout: 45_000,
  });
  const changes = page.locator('#versionRowMenu [data-command-id="version.changes"]');

  // THE REAL ROUTE, step one: CLICK THE ROW. This is the gesture the old spec
  // skipped and the one that made the feature report "No differences" forever.
  //
  // It is the MIDDLE row and not the head, and that is measured rather than
  // chosen: `openPreview` returns early on the head — "the head IS the document
  // on screen. Previewing it would swap the live session for a byte-identical
  // copy and throw away the caret for nothing" — so a click on the head opens no
  // preview and cannot create the condition. Every other row does. (This test
  // failed on exactly that line first, against the head, which is how the
  // narrowing was done.)
  await rows.nth(1).locator(".version-item-entry").click();
  await expect(
    page.locator("#versionPreviewBanner"),
    "no preview, so the live document was never replaced and this run cannot see the defect",
  ).toBeVisible({ timeout: 45_000 });

  await rows.nth(1).locator(".version-item-menu").click();
  await expect(page.locator("#versionRowMenu")).toBeVisible();
  await expect(changes).toBeEnabled();
  await changes.click();

  await expect(page.locator("#comparePanel")).toBeVisible();
  const total = page.locator("#compareBody [data-compare-total]");
  await expect(
    total,
    'the comparison produced no count at all — "No differences", which is the defect',
  ).toBeVisible({ timeout: 45_000 });
  const found = Number(await total.getAttribute("data-compare-total"));
  // NON-ZERO, on a pair that is KNOWN to differ by one typed sentence. This is
  // the assertion the old one refused to make.
  expect(
    found,
    'the middle version contains "Alpha only in the middle version. " and its ' +
      "predecessor — the import baseline — does not, so a comparison of the two " +
      "cannot find nothing",
  ).toBeGreaterThan(0);

  // AND IT FOUND THE RIGHT DIFFERENCE, not merely some difference. A comparison
  // run against the wrong side, or with the sides swapped, would also report a
  // non-zero count — so the text is named.
  await expect(page.locator("#compareBody")).toContainText("Alpha only in the middle version.");
  await expect(
    page.locator("#compareBody"),
    "Beta is in NEITHER of the two versions being compared, so naming it means " +
      "the comparison is against the wrong side — the live document, which is the defect",
  ).not.toContainText("Beta only in the newest version.");

  // READ-ONLY, which is the second half of the fix (`docs/139` §9.4, restored for
  // this route by ADR-062). `[data-compare-marked]` is rendered if and only if
  // the sidecar was applied to the live document through `applyDiffAsRevisions`
  // — `compare-on-canvas.spec.mjs` asserts it VISIBLE for Review ▸ Compare, which
  // keeps ADR-061. Version history must not write into the reader's document:
  // no competitor does, and the reader asked a question about the past.
  await expect(
    page.locator("#compareBody [data-compare-marked]"),
    "version history wrote tracked changes into the live document",
  ).toHaveCount(0);
  await expect(page.locator("#reviewShowChangesBtn")).toHaveAttribute("aria-pressed", "false");
  // AND IT IS SAID, not merely true. An absence assertion alone would pass over a
  // panel that is read-only and never mentions it — the reader would be left to
  // infer, from a surface whose other route writes into their document, that this
  // one did not. Both halves are needed for either to be worth anything.
  await expect(
    page.locator("#compareBody [data-compare-read-only]"),
    "the panel never tells the reader their document was left alone",
  ).toHaveCount(1);
  await expect(page.locator("#compareBody")).toContainText(/Read-only/i);

  // IT IS A UNIFIED DIFF, and this is the part the owner asked for: "diff should
  // be like how GitHub diff appears on a PR — things added or removed, on that
  // changes, while you can expect to see more."
  //
  // So all three parts are asserted, not just the changed lines. A panel that
  // rendered added/removed rows and no context and no expand control would be a
  // list with colours on it, which is what this replaced.
  const diff = page.locator("#compareBody [data-compare-diff]");
  await expect(diff).toBeVisible();
  // The HUNK header: where in the document this run of changes is.
  await expect(diff.locator('[data-compare-row="hunk"]').first()).toContainText(/Paragraph \d+/);
  // An ADDED line, carrying the inserted sentence. (This pair differs by an
  // insertion, so there is no removed line to assert and none is claimed.)
  await expect(diff.locator('[data-compare-row="add"]').first()).toContainText(
    "Alpha only in the middle version.",
  );
  // CONTEXT: unchanged blocks around the change, which are in NEITHER side's
  // sidecar — a change record names only what changed — so a context row with
  // real text in it is the proof that `blockTextAt` is wired and that the two
  // parsed sides survived the comparison completing.
  const context = diff.locator('[data-compare-row="context"]');
  await expect(context).not.toHaveCount(0);
  const contextText = (await context.allInnerTexts()).join(" ").trim();
  expect(
    contextText.length,
    "every context row is empty, so the context is a placeholder and not the document",
  ).toBeGreaterThan(0);

  // THE EXPAND CONTROLS ARE THERE, one per side of the hunk, and the one that
  // points off the top of the document is disabled WITH ITS REASON rather than
  // enabled and doing nothing when pressed — `SKILL` §10. The edit in this
  // fixture is at `Ctrl+Home`, so the hunk is at the first block and "above" is
  // genuinely the end of the document; that is asserted rather than worked
  // around. The REVEAL half is asserted in the test below, on a fixture whose
  // paragraph count is written in this file.
  await expect(diff.locator("[data-compare-expand]")).not.toHaveCount(0);
  const above = diff.locator('[data-compare-expand="before"]').first();
  await expect(above).toBeDisabled();
  await expect(above).toHaveAttribute("title", /no more of the document/i);

  // THE VERSION PANEL IS REOPENED, and that is not scaffolding: both surfaces
  // live in the right-hand rail and opening Compare takes it, so the version
  // rows are genuinely not on screen now. Measured — the ⋮ resolved and stayed
  // "not visible" for the whole timeout. The two menu-state assertions below
  // therefore need the panel back, and they are deliberately AFTER the
  // comparison rather than before it so that a run against the old behaviour
  // fails on the defect ("no count at all") and not on a corollary.
  // Closing Compare is the gesture, and it is the honest one: `file.versionHistory`
  // is a TOGGLE and the version panel is still open in STATE while Compare covers
  // it, so driving the command once closes it rather than revealing it (measured —
  // "#versionPanel … unexpected value hidden"). Closing Compare is also what
  // releases the two parsed sides the unified diff was reading context from.
  await page.locator("#compareClose").click();
  await expect(page.locator("#comparePanel")).toBeHidden();
  await expect(page.locator("#versionPanel")).toBeVisible();
  await expect(rows).toHaveCount(3, { timeout: 45_000 });

  // THE EARLIEST ROW HAS NO PREDECESSOR, so it is disabled WITH ITS REASON
  // rather than enabled and silently useless — `SKILL` §10, never a dead control.
  await rows.last().locator(".version-item-menu").click();
  await expect(page.locator("#versionRowMenu")).toBeVisible();
  await expect(changes).toBeDisabled();
  await expect(changes).toHaveAttribute("title", /earliest version/i);
  await expect(changes).not.toHaveAttribute("title", /not built yet/i);
  await page.keyboard.press("Escape");

  // AND THE HEAD ROW IS NOW LIVE, which is the other half of the behaviour
  // change. It used to refuse with "comparing it with itself" — true only of a
  // comparison against the document on screen. Against its predecessor the head
  // is the most useful row in the panel: it answers "what changed in the latest
  // save?", which is the question Google's history answers first.
  await rows.first().locator(".version-item-menu").click();
  await expect(page.locator("#versionRowMenu")).toBeVisible();
  await expect(changes).toBeEnabled();
  await expect(changes).not.toHaveAttribute("title", /comparing it with itself/i);
  expect(consoleErrors).toEqual([]);
});

test("every entry in the list names what it is about, never only its kind", async ({
  page,
  consoleErrors,
}) => {
  // MEASURED IN CHROMIUM ON 2026-10-04, through this exact route: bold one word
  // in the demo document and compare it with its own import baseline.
  //
  //   <li data-compare-kind="formatting"><span class="compare-kind">Reformatted</span></li>
  //   <li data-compare-kind="property" data-compare-change-family="object">
  //     <span class="compare-kind">Property changed</span></li>
  //   <li data-compare-kind="property" data-compare-change-family="section">
  //     <span class="compare-kind">Property changed</span>
  //     <span class="compare-where">in the document's definitions</span></li>
  //
  // Three of four rows named nothing. The owner's words on this surface were
  // "what the fuck will i understand from this", and the markup is why.
  //
  // THE CONDITION IS A FORMATTING-ONLY CHANGE, deliberately: a text edit already
  // rendered its excerpt, so a test that only inserted a paragraph passed over
  // the defect — which is exactly what the four tests above do.
  //
  // THE BOLD IS NOW SAVED, which it was not before, and it had to change with
  // ADR-062. The comparison used to run against the LIVE document, so an unsaved
  // bold was in it; it now runs between two stored versions, so a bold that was
  // never saved is in neither side and the formatting condition this test needs
  // would not exist. Three versions: the import baseline, the typed sentence,
  // and the bold.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.type("Formatted. ");
  await saveDocument(page);

  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  for (let i = 0; i < 8; i += 1) await page.keyboard.press("Shift+ArrowRight");
  await page.locator("#bold").click();
  await saveDocument(page);

  await runAppMenuCommand(page, "file", "file.versionHistory");
  const rows = page.locator("#versionPanelBody .version-item");
  await expect(rows, "three versions, or there is nothing to compare against").toHaveCount(3, {
    timeout: 45_000,
  });
  // THE NEWEST row, not the oldest. ADR-062 compares against a PREDECESSOR, so
  // the oldest row has nothing before it and refuses (asserted in the test
  // above); the newest holds the bold and its predecessor does not, which is the
  // formatting-only difference this test is about.
  await rows.first().locator(".version-item-menu").click();
  const changes = page.locator('#versionRowMenu [data-command-id="version.changes"]');
  await expect(changes).toBeEnabled();
  await changes.click();
  await expect(page.locator("#compareBody [data-compare-total]")).toBeVisible({
    timeout: 45_000,
  });

  // `li[data-compare-kind]` — the CHANGE rows, which is what this rule is about.
  // The unified diff puts hunk headers, context blocks and expand controls in the
  // same list, and those are not entries about a change: only a change row
  // carries `data-compare-kind`, and selecting on it keeps the guarantee exactly
  // ("every entry names what it is about") while naming the rows it is a
  // guarantee over.
  const entries = page.locator("#compareBody .compare-changes li[data-compare-kind]");
  // NON-VACUITY: a comparison that found nothing would make every assertion
  // below pass over an empty list, and a formatting change is exactly the kind
  // this build might have failed to detect.
  await expect(entries, "no entries, so the assertions below prove nothing").not.toHaveCount(0);
  const kinds = await entries.locator(".compare-kind").allInnerTexts();
  const whole = await entries.allInnerTexts();
  expect(whole).toHaveLength(kinds.length);

  // THE RULE: every entry says more than its kind. The row is kind + one of
  // (text excerpt | typed field paths | bracketed object name), and a row that
  // carries only the kind label is the defect.
  for (const [index, text] of whole.entries()) {
    const kind = kinds[index].trim();
    const rest = text.replace(kind, "").replace(/\s+/g, " ").trim();
    expect(
      rest.length,
      `entry ${index + 1} reads only "${kind}" — it says a change happened and ` +
        "refuses to say what changed",
    ).toBeGreaterThan(0);
  }

  // And at least one of them is a formatting or property row, so the condition
  // this test needs really was created rather than silently becoming a text-only
  // comparison.
  const rowKinds = await entries.evaluateAll((items) =>
    items.map((item) => item.dataset.compareKind ?? ""),
  );
  expect(rowKinds.some((kind) => kind === "formatting" || kind === "property")).toBe(true);

  // The corrected sentence, and not the one that said a finished comparison had
  // not finished. `record.rs`: `complete` is false whenever `findings` is
  // non-empty, which an inline object in the demo document makes true.
  await expect(page.locator("#compareBody")).not.toContainText(/did not finish/i);
  expect(consoleErrors).toEqual([]);
});

test("the unified diff expands its context, and an entry scrolls to its change", async ({
  page,
  consoleErrors,
}) => {
  // THE FIXTURE'S PARAGRAPH COUNT IS WRITTEN IN THIS FILE, which is the point of
  // doing this on the file route rather than on the demo document: the hunk sits
  // at paragraph 20 of 40, so there are unchanged blocks in BOTH directions and
  // both expand controls are genuinely live. On the version-history fixture the
  // edit is at `Ctrl+Home`, where "above" is the end of the document — a correct
  // disabled control, and no way to test the reveal.
  const lines = (changed) =>
    Array.from({ length: 40 }, (_, index) =>
      index === 19 ? changed : `Paragraph number ${index + 1} is unchanged`,
    ).join("\n");
  await openText(page, textFile("mine.txt", `${lines("The twentieth paragraph as I wrote it")}\n`));
  await compareAgainst(page, textFile("theirs.txt", `${lines("The twentieth paragraph as THEY wrote it")}\n`), {
    via: "rail",
  });

  const diff = page.locator("#compareBody [data-compare-diff]");
  await expect(diff).toBeVisible();
  // One hunk, named by position. 40 paragraphs differing in exactly one place
  // cannot produce two hunks, so this also catches a grouping rule that split
  // on every change.
  await expect(diff.locator('[data-compare-row="hunk"]')).toHaveCount(1);
  await expect(diff.locator('[data-compare-row="hunk"]')).toContainText("Paragraph 20");

  // REMOVED then ADDED, GitHub's order, with the real text on both sides — the
  // half a summary count cannot show. `openText` opened MINE, so mine is the
  // right-hand (newer) side and theirs is the left: an insertion is what this
  // document has.
  const removed = diff.locator('[data-compare-row="del"]');
  const added = diff.locator('[data-compare-row="add"]');
  await expect(removed).toHaveCount(1);
  await expect(added).toHaveCount(1);
  await expect(removed).toContainText("as THEY wrote it");
  await expect(added).toContainText("as I wrote it");

  // CONTEXT ON BOTH SIDES, with the actual neighbouring paragraphs in it. These
  // are in NEITHER side's sidecar — a change record names only what changed — so
  // this is the proof that `blockTextAt` is wired to the right side and the
  // right path. 19 and 21 bracket the change; naming them is what distinguishes
  // real context from three blank rows.
  const context = diff.locator('[data-compare-row="context"]');
  await expect(context).toHaveCount(6, { timeout: 45_000 });
  await expect(diff).toContainText("Paragraph number 19 is unchanged");
  await expect(diff).toContainText("Paragraph number 21 is unchanged");
  await expect(
    diff,
    "paragraph 16 is four blocks away and is showing before anything was expanded",
  ).not.toContainText("Paragraph number 16 is unchanged");

  // AND YOU CAN SEE MORE — the owner's "while you can expect to see more". Both
  // controls are live here, and pressing one must actually add rows: a button
  // that renders and does nothing is the dead control `SKILL` §10 forbids.
  const above = diff.locator('[data-compare-expand="before"]');
  const below = diff.locator('[data-compare-expand="after"]');
  await expect(above).toBeEnabled();
  await expect(below).toBeEnabled();
  // THE SIZER IS THE ROW ARRAY and the DOM is the window, so growth is measured
  // on the sizer rather than by counting rendered rows. Counting rows is what
  // this test tried first and it is the wrong instrument: 26 context rows do not
  // fit in a 46vh scroller, so an exact count asserts the viewport's height.
  const sizerHeight = () =>
    diff.locator(".compare-diff-sizer").evaluate((element) => element.offsetHeight);
  const start = await sizerHeight();
  await above.click();
  await expect(
    diff,
    "the expand control was enabled and revealed nothing above",
  ).toContainText("Paragraph number 16 is unchanged");
  const expanded = await sizerHeight();
  expect(expanded, "the row array did not grow when context was revealed").toBeGreaterThan(start);
  await below.click();
  expect(await sizerHeight(), "expanding below did not grow the row array").toBeGreaterThan(
    expanded,
  );

  // THE WINDOW IS NOT THE LIST, and this is the assertion that fails if the
  // renderer goes back to one node per change in one synchronous loop. The row
  // array is `sizer / 22` rows tall (`DIFF_ROW_HEIGHT`, which JS writes onto the
  // container as `--diff-row-h` so the arithmetic and the stylesheet cannot
  // disagree); the DOM holds strictly fewer.
  const rows2 = Math.round((await sizerHeight()) / 22);
  const rendered = await diff.locator(".compare-changes > li").count();
  expect(rows2, "the sizer is not sized from the whole row array").toBeGreaterThan(28);
  expect(
    rendered,
    `the DOM holds ${rendered} of ${rows2} rows — the window is rendering the whole list`,
  ).toBeLessThan(rows2);

  // AN ENTRY SCROLLS TO ITS CHANGE. This is the route where it CAN: Review ▸
  // Compare's right-hand side IS the open document, so the change's `right.path`
  // addresses a block on screen and `nodeAtStoryPath` turns it into that
  // paragraph's real NodeId. `DiffAnchor.node` is never used — it addresses a
  // throwaway parse whose counter restarted at 1, so it would land on an
  // unrelated paragraph and report nothing wrong.
  const goto = diff.locator("[data-compare-goto]").first();
  await expect(goto).toBeVisible();
  await goto.click();
  // The EFFECT, not the mechanism: the caret is now in the paragraph the change
  // is about. Asserted through the selection rather than through a scroll
  // position, because a scroll offset is a measured number and the guarantee is
  // "the reader is taken to that paragraph".
  await expect
    .poll(async () => page.evaluate(() => window.getSelection()?.toString() ?? ""), {
      timeout: 45_000,
    })
    .toContain("as I wrote it");
  expect(consoleErrors).toEqual([]);
});
