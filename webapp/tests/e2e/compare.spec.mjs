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

test("Show changes is live on a stored version and refused on the head, with the reason", async ({
  page,
  consoleErrors,
}) => {
  // Google Docs' route into a comparison, and the row whose disabled reason used
  // to say the structural diff "is not built yet" — which was wrong about which
  // half was missing: the diff was built and the PANEL was not.
  await gotoEditor(page);
  // TWO rows are needed, and they have to be two real versions: the import
  // baseline plus a save of a document that actually changed. A Save over an
  // unmodified document reports `history.unchanged` and writes nothing (the
  // owner's rule, `docs/139` §18 q3 as revised), so typing first is not padding —
  // without it `rows.last()` is `rows.first()` and this test would assert the
  // head's refusal twice and call it a pass.
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.type("Compared. ");
  await saveDocument(page);
  await runAppMenuCommand(page, "file", "file.versionHistory");
  await expect(page.locator("#versionPanel")).toBeVisible();

  const rows = page.locator("#versionPanelBody .version-item");
  await expect(rows, "two versions, or the head's refusal is asserted twice").toHaveCount(2, {
    timeout: 45_000,
  });
  const changes = page.locator('#versionRowMenu [data-command-id="version.changes"]');

  // The HEAD row refuses, because the head IS the document on screen: comparing
  // it with itself would report nothing. Disabled WITH a reason — and
  // specifically not the old sentence, which is the line that fails if the stale
  // claim comes back.
  await rows.first().locator(".version-item-menu").click();
  await expect(page.locator("#versionRowMenu")).toBeVisible();
  await expect(changes).toBeDisabled();
  await expect(changes).toHaveAttribute("title", /comparing it with itself/i);
  await expect(changes).not.toHaveAttribute("title", /not built yet/i);
  await page.keyboard.press("Escape");

  // Any OTHER row is live. The oldest, which is the one the version-history spec
  // drives too — and the row this whole lane exists to stop lying about.
  await rows.last().locator(".version-item-menu").click();
  await expect(page.locator("#versionRowMenu")).toBeVisible();
  await expect(changes).toBeEnabled();
  await changes.click();

  // THE EFFECT: it opens the comparison surface and finishes a real comparison
  // against that checkpoint — the same panel Review ▸ Compare opens, named by
  // WHEN the version was rather than by a file name every version shares.
  await expect(page.locator("#comparePanel")).toBeVisible();
  await expect(page.locator("#compareBody")).toContainText(/Compared with|No differences/i, {
    timeout: 45_000,
  });
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
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.type("Formatted. ");
  await saveDocument(page);

  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  for (let i = 0; i < 8; i += 1) await page.keyboard.press("Shift+ArrowRight");
  await page.locator("#bold").click();

  await runAppMenuCommand(page, "file", "file.versionHistory");
  const rows = page.locator("#versionPanelBody .version-item");
  await expect(rows, "two versions, or there is nothing to compare against").toHaveCount(2, {
    timeout: 45_000,
  });
  await rows.last().locator(".version-item-menu").click();
  const changes = page.locator('#versionRowMenu [data-command-id="version.changes"]');
  await expect(changes).toBeEnabled();
  await changes.click();
  await expect(page.locator("#compareBody [data-compare-total]")).toBeVisible({
    timeout: 45_000,
  });

  const entries = page.locator("#compareBody .compare-changes li");
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
