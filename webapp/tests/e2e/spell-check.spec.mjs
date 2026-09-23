// Spelling, through the surfaces a user actually has (docs/114, `109` HF-035).
//
// Five things this file is shaped by, from `docs/114` §7 and SKILL.md §4:
//
//   * **the positive case is asserted first.** "No squiggle on a correct word"
//     passes when the feature does not exist; the first assertion anywhere here
//     is that a misspelling PRODUCES a marker with the word on it.
//   * **assert through the paint layer.** The document text is not in `#pages`
//     — the page is a canvas. `.overlay .spell-error` is a real element in the
//     same layer `.overlay .caret` and `.overlay .highlight` already live in,
//     and it carries `data-spell-word` so an assertion can name the word.
//   * **`page.keyboard.insertText`, never `keyboard.type`** — `type` is
//     `preventDefault`ed for printable characters and the element under test
//     never receives them.
//   * **page 5 is a different question from page 1.** The scan is windowed, so
//     a spec that only ever looks at the first viewport proves nothing about
//     the mechanism; the windowedness itself is asserted, not assumed.
//   * **never `@` as a probe marker** — the fidelity corpus has
//     `info@docscentre.com`.

import {
  test,
  expect,
  documentPageCount,
  gotoEditor,
  openCommandPalette,
  pageSheet,
  MOD,
  runAppMenuCommand,
  setReviewMode,
  stableBox,
  runFilePageCommand,
} from "./fixtures.mjs";
import { GRAMMAR_DOUBLED, makeSpellingDocx, spellingTypoForPage } from "./large-docx.mjs";

const DOCX_MIME =
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/** Opens the spelling fixture and waits for the document, not for a clock. */
async function openSpellingDocument(page, { pages: pageCount = 6, language = "" } = {}) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.setInputFiles("#file", {
    name: "spelling.docx",
    mimeType: DOCX_MIME,
    buffer: Buffer.from(makeSpellingDocx(pageCount, language)),
  });
  await page.waitForFunction(
    () =>
      document.getElementById("status")?.textContent !== null &&
      document.querySelectorAll("canvas.page").length > 0,
    null,
    { timeout: 45_000 },
  );
  await expect.poll(() => documentPageCount(page)).toBe(pageCount);
}

const marker = (word) => `.overlay .spell-error[data-spell-word="${word}"]`;
const grammarMark = (rule) => `.overlay .grammar-error[data-grammar-rule="${rule}"]`;

/**
 * Right-clicks a squiggle at its own coordinates.
 *
 * Not `locator.click()`: the marker is `pointer-events: none` on purpose, so
 * the event has to reach the page's own hit-testing underneath it. And not
 * `boundingBox()` directly — the overlay is rebuilt on every repaint, so a box
 * read a frame after the count assertion can come back null. `stableBox`
 * retries, which is the same flake the fixture was written for.
 */
async function rightClickSquiggle(page, locator) {
  const box = await stableBox(locator);
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2, {
    button: "right",
  });
  const menu = page.locator(".editor-context-menu");
  await expect(menu).toBeVisible();
  return menu;
}

test("a misspelled word is squiggled, and the marker names the word", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);

  // THE positive case. Everything else in this file is worthless without it.
  await expect(page.locator(marker(spellingTypoForPage(1)))).toHaveCount(1, {
    timeout: 20_000,
  });

  // And the prose around it is not squiggled. The fixture puts exactly one
  // wrong word on each page and the window covers more than one page, so the
  // assertion is on WHICH words are flagged, not how many: every marker on
  // screen must be one of the deliberate typos. A checker that flagged
  // ordinary English would fail here and nowhere else.
  const flagged = await page.locator(".overlay .spell-error").evaluateAll((els) => [
    ...new Set(els.map((el) => el.dataset.spellWord)),
  ]);
  const deliberate = [1, 2, 3, 4, 5, 6].map((n) => spellingTypoForPage(n));
  expect(flagged.filter((word) => !deliberate.includes(word))).toEqual([]);
  expect(flagged).toContain(spellingTypoForPage(1));

  // The marker must not eat the pointer — the right-click has to reach the
  // page's own hit-testing or caret placement breaks (REVIEW-GAP-005).
  const events = await page
    .locator(marker(spellingTypoForPage(1)))
    .evaluate((el) => getComputedStyle(el).pointerEvents);
  expect(events).toBe("none");

  expect(consoleErrors).toEqual([]);
});

test("the scan is windowed: page 5's misspelling is flagged only once it is on screen", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);
  const far = spellingTypoForPage(5);

  await expect(page.locator(marker(spellingTypoForPage(1)))).toHaveCount(1, {
    timeout: 20_000,
  });
  // Nothing outside the window is painted. NOTE what this does and does not
  // prove: `place()` returns null for a page with no sheet, so this assertion
  // stays green even if the scan walked the whole document — removing the page
  // bound from the walk was mutation-tested here and this spec did not notice.
  // The scan's actual EXTENT is only observable from inside, and is guarded in
  // `tests/spell_check.test.mjs` by counting what the engine is asked.
  await expect(page.locator(marker(far))).toHaveCount(0);

  await pageSheet(page, 5);
  await expect(page.locator(marker(far))).toHaveCount(1, { timeout: 20_000 });
  expect(consoleErrors).toEqual([]);
});

test("right-click offers suggestions, and picking one is a single undoable edit", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);
  const typo = spellingTypoForPage(1); // "sentance"
  const squiggle = page.locator(marker(typo));
  await expect(squiggle).toHaveCount(1, { timeout: 20_000 });

  const menu = await rightClickSquiggle(page, squiggle.first());

  // Word and Docs both LEAD with the suggestions; anything above them makes the
  // user read past the reason they right-clicked.
  const first = menu.locator("[data-command-id]").first();
  await expect(first).toHaveAttribute("data-command-id", "spell.suggestion.0");
  await expect(first).toHaveText("sentence");
  for (const id of ["spell.ignoreOnce", "spell.ignoreAll", "spell.addToDictionary"]) {
    await expect(menu.locator(`[data-command-id="${id}"]`)).toBeEnabled();
  }

  await first.click();
  await expect(menu).toBeHidden();
  // The replacement went through `replaceRanges`, so the squiggle is gone and
  // the word is really in the document — asked through Find, which reads the
  // engine rather than the DOM.
  await expect(page.locator(marker(typo))).toHaveCount(0, { timeout: 20_000 });

  // ONE undo brings the misspelling back: the same single-action guarantee
  // Replace All has, because it is the same call.
  await page.keyboard.press(`${MOD}+z`);
  // Undo leaves the caret in the restored word, and the word under the caret is
  // deliberately not flagged (§3 — Word's behaviour, and the reason typing costs
  // nothing). Move away, as a user would, and the squiggle is back.
  await page.keyboard.press(`${MOD}+Home`);
  await expect(page.locator(marker(typo))).toHaveCount(1, { timeout: 20_000 });
  expect(consoleErrors).toEqual([]);
});

test("Ignore all clears the word, and Add to dictionary survives a reload", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);
  await page.locator("#pages").click({ position: { x: 60, y: 60 } });
  // A word no dictionary can know, typed in — `insertText`, because
  // `keyboard.type` is preventDefaulted for printable characters here.
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.insertText(" Qzxworden and Qzxworden again.");
  // The caret is inside the last word, so the FIRST occurrence is what gets
  // flagged. Move away so both are.
  await page.keyboard.press(`${MOD}+Home`);

  const invented = page.locator(`${marker("Qzxworden")}`);
  await expect(invented).toHaveCount(0); // it is on the last page, not this one
  await pageSheet(page, 6);
  await expect(invented.first()).toBeVisible({ timeout: 20_000 });
  const occurrences = await invented.count();
  expect(occurrences).toBeGreaterThanOrEqual(1);

  const menu = await rightClickSquiggle(page, invented.first());
  await menu.locator('[data-command-id="spell.addToDictionary"]').click();

  await expect(page.locator(marker("Qzxworden"))).toHaveCount(0, { timeout: 20_000 });
  await expect(page.locator("#status")).toContainText("Added", { timeout: 20_000 });

  // The whole point of the personal dictionary: it is still there next time.
  await openSpellingDocument(page);
  await page.locator("#pages").click({ position: { x: 60, y: 60 } });
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.insertText(" Qzxworden here too.");
  await page.keyboard.press(`${MOD}+Home`);
  await pageSheet(page, 6);
  await expect(page.locator(marker(spellingTypoForPage(6)))).toHaveCount(1, {
    timeout: 20_000,
  });
  await expect(page.locator(marker("Qzxworden"))).toHaveCount(
    0,
    "a word added to the personal dictionary is not flagged after a reload",
  );
  expect(consoleErrors).toEqual([]);
});

test("Ignore all silences a word for the session without persisting it", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);
  const typo = spellingTypoForPage(1);
  const squiggle = page.locator(marker(typo));
  await expect(squiggle).toHaveCount(1, { timeout: 20_000 });

  const menu = await rightClickSquiggle(page, squiggle.first());
  await page
    .locator('.editor-context-menu [data-command-id="spell.ignoreAll"]')
    .click();
  await expect(page.locator(marker(typo))).toHaveCount(0, { timeout: 20_000 });

  // Word does not persist Ignore All, and neither do we: a new document brings
  // the word back. (Add to dictionary is the one that persists.)
  await openSpellingDocument(page);
  await expect(page.locator(marker(typo))).toHaveCount(1, { timeout: 20_000 });
  expect(consoleErrors).toEqual([]);
});

test("the off switch is reachable from two surfaces and is remembered", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);
  const typo = spellingTypoForPage(1);
  await expect(page.locator(marker(typo))).toHaveCount(1, { timeout: 20_000 });

  // Surface 1 — the Tools menu. `menu_taxonomy.test.mjs` holds the menu and the
  // command registry to agreement; this proves the row actually works.
  await runAppMenuCommand(page, "review", "tools.spellCheck");
  await expect(page.locator(".overlay .spell-error")).toHaveCount(0);
  await expect(page.locator("#status")).toHaveText("Spell check off");

  // Remembered across a reload — a preference that resets is not one.
  await openSpellingDocument(page);
  await expect(page.locator(".overlay .spell-error")).toHaveCount(0);

  // Surface 2 — the command palette. The command must still be ENABLED while
  // off, or there is no way back (SKILL.md §10, never a dead control).
  await openCommandPalette(page);
  await page.locator("#cmdInput").fill("spell check");
  const row = page.locator("#cmdList [data-command-id='tools.spellCheck']");
  await expect(row).toBeVisible();
  await expect(row).toContainText("off");
  await row.click();
  await expect(page.locator(marker(typo))).toHaveCount(1, { timeout: 20_000 });

  // Surface 3 — the Settings panel checkbox, reflecting the same state.
  await runFilePageCommand(page, "view.settings");
  await expect(page.locator("#spellCheckToggle")).toBeChecked();
  expect(consoleErrors).toEqual([]);
});

test("a language with no dictionary is NAMED, not silently shown as clean", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page, { language: "fr-FR" });
  // The words are still English misspellings; what changed is the declared
  // language. Nothing may be flagged — and the user has to be told why, or a
  // document nobody is checking is indistinguishable from a correct one.
  await expect(page.locator("#status")).toHaveText("No spelling dictionary for fr-FR", {
    timeout: 20_000,
  });
  await expect(page.locator(".overlay .spell-error")).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("in Viewing mode the corrections refuse with a reason instead of silently failing", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);
  const typo = spellingTypoForPage(1);
  const squiggle = page.locator(marker(typo));
  await expect(squiggle).toHaveCount(1, { timeout: 20_000 });
  await setReviewMode(page, "viewing");
  await expect(squiggle).toHaveCount(1, "reading a document still shows its spelling");

  const menu = await rightClickSquiggle(page, squiggle.first());
  const suggestion = menu.locator('[data-command-id="spell.suggestion.0"]');
  await expect(suggestion).toBeVisible();
  await expect(suggestion).toBeDisabled();
  // Ignoring is a host-side decision, not a document mutation, so it stays
  // available: a reader looking at a false positive can still dismiss it.
  await expect(menu.locator('[data-command-id="spell.ignoreAll"]')).toBeEnabled();
  expect(consoleErrors).toEqual([]);
});

// ---- Grammar (the owner rated this above spelling, 2026-09-23) ----------------

test("a grammar error gets its own blue mark, distinct from a spelling one", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);
  const doubled = page.locator(grammarMark("doubled-word"));
  await expect(doubled.first()).toBeVisible({ timeout: 20_000 });

  // The two marks are different elements with different colours, because the
  // reader has to tell "this word is misspelled" from "this sentence is wrong"
  // without opening anything.
  await expect(page.locator(marker(spellingTypoForPage(1)))).toHaveCount(1);
  const [spellColour, grammarColour] = await page.evaluate(() => {
    const read = (selector) => {
      const el = document.querySelector(selector);
      return el ? getComputedStyle(el, "::after").backgroundImage : "";
    };
    return [read(".overlay .spell-error"), read(".overlay .grammar-error")];
  });
  expect(spellColour).not.toBe("");
  expect(grammarColour).not.toBe("");
  expect(grammarColour).not.toBe(spellColour);
  expect(consoleErrors).toEqual([]);
});

test("right-clicking a grammar mark explains the rule, fixes it, and can silence it", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);
  const doubled = page.locator(grammarMark("doubled-word"));
  await expect(doubled.first()).toBeVisible({ timeout: 20_000 });

  // The fixture repeats the grammar line on every page, and the window covers
  // more than one — so the assertion is that THIS one is fixed, not that none
  // is left. Counting is how a multi-page window is read honestly.
  const before = await doubled.count();
  expect(before).toBeGreaterThan(0);

  const menu = await rightClickSquiggle(page, doubled.first());
  const explain = menu.locator('[data-command-id="grammar.explain"]');
  await expect(explain).toBeVisible();
  await expect(explain).toBeDisabled();
  await expect(explain).toContainText("repeated");

  await menu.locator('[data-command-id="grammar.suggestion.0"]').click();
  await expect(menu).toBeHidden();
  await expect(page.locator(grammarMark("doubled-word"))).toHaveCount(before - 1, {
    timeout: 20_000,
  });

  // One undo, through the same replaceRanges path spelling uses.
  await page.keyboard.press(`${MOD}+z`);
  await page.keyboard.press(`${MOD}+Home`);
  await expect(page.locator(grammarMark("doubled-word"))).toHaveCount(before, {
    timeout: 20_000,
  });

  const again = await rightClickSquiggle(page, page.locator(grammarMark("doubled-word")).first());
  await again.locator('[data-command-id="grammar.ignoreRule"]').click();
  await expect(page.locator(grammarMark("doubled-word"))).toHaveCount(0, { timeout: 20_000 });
  // ...and only that rule: the spelling mark beside it is untouched.
  await expect(page.locator(marker(spellingTypoForPage(1)))).toHaveCount(1);
  expect(consoleErrors).toEqual([]);
});

test("grammar and spelling are independent switches, both remembered", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);
  await expect(page.locator(grammarMark("doubled-word")).first()).toBeVisible({
    timeout: 20_000,
  });

  // Spelling off, grammar still on — the owner rated grammar the more
  // important of the two, so it must not be reachable only via spelling.
  await runAppMenuCommand(page, "review", "tools.spellCheck");
  await expect(page.locator(".overlay .spell-error")).toHaveCount(0);
  await expect(page.locator(grammarMark("doubled-word")).first()).toBeVisible();

  await runAppMenuCommand(page, "review", "tools.grammarCheck");
  await expect(page.locator(".overlay .grammar-error")).toHaveCount(0);
  await expect(page.locator("#status")).toHaveText("Grammar check off");

  // Both remembered across a reload, independently.
  await openSpellingDocument(page);
  await expect(page.locator(".overlay .spell-error")).toHaveCount(0);
  await expect(page.locator(".overlay .grammar-error")).toHaveCount(0);

  await runAppMenuCommand(page, "review", "tools.grammarCheck");
  await expect(page.locator(grammarMark("doubled-word")).first()).toBeVisible({
    timeout: 20_000,
  });
  await expect(page.locator(".overlay .spell-error")).toHaveCount(0, {
    timeout: 5_000,
  });
  // Put spelling back so the stored preference does not leak into other specs.
  await runAppMenuCommand(page, "review", "tools.spellCheck");
  expect(consoleErrors).toEqual([]);
});

// ---- The shipped glossary -------------------------------------------------------

test("our own product names are never flagged, and are not the user's words", async ({
  page,
  consoleErrors,
}) => {
  await openSpellingDocument(page);
  await page.locator("#pages").click({ position: { x: 60, y: 60 } });
  await page.keyboard.press(`${MOD}+End`);
  // Typed, not in the fixture, so this really goes through the checker.
  await page.keyboard.insertText(" opendoc and opencalc and CasualOffice and OOXML.");
  await page.keyboard.press(`${MOD}+Home`);
  await pageSheet(page, 6);
  // The page's own deliberate typo is still flagged, which is what proves the
  // checker ran at all on this page.
  await expect(page.locator(marker(spellingTypoForPage(6)))).toHaveCount(1, {
    timeout: 20_000,
  });
  for (const name of ["opendoc", "opencalc", "CasualOffice", "OOXML"]) {
    await expect(page.locator(marker(name))).toHaveCount(0, `${name} must not be flagged`);
  }
  expect(consoleErrors).toEqual([]);
});
