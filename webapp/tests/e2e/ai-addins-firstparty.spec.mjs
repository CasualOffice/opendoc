// The first-party add-ins LanguageTool and Readability (`docs/183` §18, §19) in
// a real browser, the way LanguageTool's add-on, Grammarly and Google Docs show
// writing issues: underlined IN the text as you type, a card at the word on a
// click, a fix in one click that is one undo step. LanguageTool's server is a
// Playwright route standing in for api.languagetool.org (the add-in's declared
// origin), so nothing leaves the machine.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, setReviewMode, stableBox, MOD } from "./fixtures.mjs";

test.describe.configure({ timeout: 120_000 });

const LT = "org.casualoffice.languagetool";
const RD = "org.casualoffice.readability";
const TYPO = "Zorbly";

async function openManager(page) {
  await page.locator("#settingsBtn").click();
  await page.locator("#settingsAddinsBtn").click();
  await expect(page.locator("#pluginsDialog")).toBeVisible();
}

function row(page, name) {
  return page.locator("#pluginsList .plugins-row", { has: page.locator("strong", { hasText: name }) });
}

/** The mock server: every "Zorbly" is a misspelling (autocorrect leaves it alone). */
async function mockLanguageTool(page) {
  const sent = [];
  await page.route("https://api.languagetool.org/v2/check", async (route) => {
    const params = new URLSearchParams(route.request().postData() ?? "");
    const text = params.get("text") ?? "";
    sent.push({ text, language: params.get("language") });
    const matches = [...text.matchAll(/\bZorbly\b/g)].map((m) => ({
      offset: m.index,
      length: 6,
      message: "Possible spelling mistake found.",
      shortMessage: "Spelling mistake",
      replacements: [{ value: "Orderly" }, { value: "Zebra" }],
      rule: { id: "MORFOLOGIK_RULE_EN_US", issueType: "misspelling", category: { id: "TYPOS", name: "Possible Typo" } },
    }));
    await route.fulfill({ contentType: "application/json", headers: { "access-control-allow-origin": "*" }, body: JSON.stringify({ matches }) });
  });
  return sent;
}

async function turnOn(page, name, { prompt }) {
  await openManager(page);
  await row(page, name).locator("button", { hasText: "Turn on" }).click();
  if (prompt) {
    await expect(page.locator("#pluginGrantDialog")).toBeVisible();
    await page.locator("#pluginGrantAllow").click();
  } else {
    await expect(page.locator("#pluginGrantDialog")).toBeHidden();
  }
  await page.locator("#pluginsDone").click();
  // Settings is still open behind Manage add-ins.
  await page.keyboard.press("Escape");
}

/** Types `text` as a new paragraph after the first one. */
async function typeParagraph(page, text) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type(text);
}

const addinMarks = (page, id) => page.locator(`.overlay [data-addin="${id}"]`);
const hits = (page, word) => page.evaluate(async (w) => (await window.opendoc.ai.read({ tool: "search", query: w })).value.hits.length, word);

async function clickMark(page, mark) {
  const box = await stableBox(mark);
  // The mark is the squiggle's box: click inside the word it sits under.
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}

test("both first-party add-ins are listed, off, and change nothing until turned on", async ({ page }) => {
  await gotoEditor(page);
  await expect(page.locator("#reviewAddinsBtn")).toBeHidden();
  await expect(page.locator("#addinIssuesStatus")).toBeHidden();
  await openManager(page);
  await expect(row(page, "LanguageTool")).toContainText("Turn on");
  await expect(row(page, "Readability")).toContainText("Turn on");
  await expect(page.locator("iframe[data-plugin]")).toHaveCount(0);
});

test("LanguageTool: type an error, it is underlined, click it, fix it in one click, undo brings both back", async ({ page }) => {
  const sent = await mockLanguageTool(page);
  await gotoEditor(page);
  await turnOn(page, "LanguageTool", { prompt: true });
  // No panel and no ribbon button: the issues are in the text.
  await expect(page.locator("#pluginPanel")).toBeHidden();
  await expect(page.locator("#reviewAddinsBtn")).toBeHidden();

  await typeParagraph(page, `${TYPO} quick brown fox. `);
  // 1. The mark appears, as you type, drawn by the host.
  const mark = addinMarks(page, LT).first();
  await expect(mark).toBeVisible({ timeout: 15_000 });
  await expect(mark).toHaveClass(/spell-error/);
  expect(sent.length).toBeGreaterThan(0);
  // One underline per word: no built-in squiggle under the add-in's.
  const doubled = await page.evaluate(() => {
    const addin = [...document.querySelectorAll(".overlay [data-addin]")].map((el) => el.getBoundingClientRect());
    return [...document.querySelectorAll(".overlay .spell-error:not([data-addin]), .overlay .grammar-error:not([data-addin])")]
      .map((el) => el.getBoundingClientRect())
      .filter((r) => addin.some((a) => Math.abs(a.left - r.left) < 2 && Math.abs(a.top - r.top) < 2)).length;
  });
  expect(doubled).toBe(0);
  await expect(page.locator("#addinIssuesStatus")).toHaveText("Writing issues: 1");

  // 2. A click on the mark opens the card at the word.
  await clickMark(page, mark);
  const card = page.locator(".addin-card");
  await expect(card).toBeVisible();
  await expect(card.locator(".review-inline-who strong")).toHaveText("Spelling");
  await expect(card.locator(".review-inline-who small")).toHaveText("LanguageTool");
  await expect(card.locator(".review-inline-body")).toHaveText("Possible spelling mistake found.");
  await expect(card.locator("button")).toHaveText(["Orderly", "Zebra", "closeIgnore", "bookAdd to dictionary"]);

  // 3. Accept: the text is fixed in place and the mark is gone.
  await card.locator("button", { hasText: /^Orderly$/ }).click();
  await expect(card).toBeHidden();
  await expect.poll(() => hits(page, "Orderly quick brown")).toBeGreaterThan(0);
  expect(await hits(page, `${TYPO} quick`)).toBe(0);
  await expect(addinMarks(page, LT)).toHaveCount(0);
  await expect(page.locator("#addinIssuesStatus")).toHaveText("Writing issues: 0");

  // 4. One undo restores the text AND the mark.
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(() => hits(page, `${TYPO} quick`)).toBeGreaterThan(0);
  await expect(addinMarks(page, LT).first()).toBeVisible({ timeout: 15_000 });
  await expect(page.locator("#addinIssuesStatus")).toHaveText("Writing issues: 1");
});

test("LanguageTool from the keyboard: ⌘' goes to the issue and opens its card, announced; the status count does too", async ({ page }) => {
  await mockLanguageTool(page);
  await gotoEditor(page);
  await turnOn(page, "LanguageTool", { prompt: true });
  await typeParagraph(page, `Then ${TYPO} end. `);
  await expect(addinMarks(page, LT).first()).toBeVisible({ timeout: 15_000 });
  await moveCaretToDocStart(page);
  await page.keyboard.press(`${MOD}+'`);
  const card = page.locator(".addin-card");
  await expect(card).toBeVisible();
  // Focus is in the card, on the first fix, so Enter applies it.
  await expect(card.locator("button").first()).toBeFocused();
  await expect(page.locator("#reviewLiveRegion")).toContainText("Issue 1 of 1: Spelling.");
  await page.keyboard.press("Escape");
  await expect(card).toBeHidden();
  // ⌥⏎ opens the card at the caret (Next selected the word).
  await page.keyboard.press("Alt+Enter");
  await expect(card).toBeVisible();
  await page.keyboard.press("Enter");
  await expect.poll(() => hits(page, "Then Orderly end")).toBeGreaterThan(0);
  // The count in the status bar goes to the next issue.
  await page.keyboard.press(`${MOD}+z`);
  await expect(page.locator("#addinIssuesStatus")).toHaveText("Writing issues: 1", { timeout: 15_000 });
  await page.locator("#addinIssuesStatus").click();
  await expect(card).toBeVisible();
});

test("LanguageTool in Suggesting mode: the fix lands as a tracked suggestion, not a direct edit", async ({ page }) => {
  const sent = await mockLanguageTool(page);
  await gotoEditor(page);
  await turnOn(page, "LanguageTool", { prompt: true });
  await typeParagraph(page, `${TYPO} cat. `);
  await expect(addinMarks(page, LT).first()).toBeVisible({ timeout: 15_000 });
  await setReviewMode(page, "suggesting");
  await clickMark(page, addinMarks(page, LT).first());
  const before = sent.length;
  await page.locator(".addin-card button", { hasText: /^Orderly$/ }).click();
  await expect.poll(async () => page.evaluate(async () => (await window.opendoc.ai.read({ tool: "trackedChanges" })).value.trackedChanges.length)).toBeGreaterThan(0);
  await expect(addinMarks(page, LT)).toHaveCount(0);
  // The deleted word stays in the paragraph as a tracked deletion; it is not
  // underlined again, even after the paragraph changes and is checked again
  // (typed in Editing, because a tracked insertion is not proofing text).
  await setReviewMode(page, "editing");
  await page.keyboard.press("End");
  await page.keyboard.type("Done.");
  await expect.poll(() => sent.length).toBeGreaterThan(before);
  await expect.poll(() => sent.at(-1).text).toContain("Done.");
  await expect(addinMarks(page, LT)).toHaveCount(0);
});

test("Readability highlights in the text, Hemingway-style, runs on the device, and its card has no fix to apply", async ({ page }) => {
  const requested = [];
  page.on("request", (request) => requested.push(request.url()));
  await gotoEditor(page);
  await turnOn(page, "Readability", { prompt: false });
  await typeParagraph(page, "The report was written by the team. It is very clear. ");
  const passive = page.locator(`.overlay .addin-highlight-passive[data-addin="${RD}"]`).first();
  await expect(passive).toBeVisible({ timeout: 15_000 });
  await expect(page.locator(`.overlay .addin-highlight-filler[data-addin="${RD}"]`).first()).toBeVisible();
  await clickMark(page, passive);
  const card = page.locator(".addin-card");
  await expect(card.locator(".review-inline-who strong")).toHaveText("Passive voice");
  await expect(card.locator("button")).toHaveText(["closeIgnore"]);
  await card.locator("button").click();
  await expect(page.locator(`.overlay .addin-highlight-passive[data-addin="${RD}"]`)).toHaveCount(0);
  const origin = new URL(page.url()).origin;
  expect(requested.filter((u) => /^https?:/.test(u) && new URL(u).origin !== origin)).toEqual([]);
});
