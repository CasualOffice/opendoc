// The first-party add-ins LanguageTool and Readability (`docs/183` §18) in a
// real browser. LanguageTool's server is a Playwright route standing in for
// api.languagetool.org (the add-in's declared origin), so nothing leaves the
// machine. Both are off until a person turns them on; both only propose, and
// what they propose is signed "<name> (add-in)", never "Casual AI".
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

test.describe.configure({ timeout: 120_000 });

async function openManager(page) {
  await page.locator("#settingsBtn").click();
  await page.locator("#settingsAddinsBtn").click();
  await expect(page.locator("#pluginsDialog")).toBeVisible();
}

function row(page, name) {
  return page.locator("#pluginsList .plugins-row", { has: page.locator("strong", { hasText: name }) });
}

async function runFromAddins(page, label) {
  await page.locator('[data-tab="review"]').click();
  await expect(page.locator("#reviewAddinsBtn")).toBeVisible();
  await page.locator("#reviewAddinsBtn").click();
  await page.locator("#addinsMenu .menu-item", { hasText: label }).click();
}

test("both first-party add-ins are listed, off, and change nothing until turned on", async ({ page }) => {
  await gotoEditor(page);
  await expect(page.locator("#reviewAddinsBtn")).toBeHidden();
  await openManager(page);
  await expect(row(page, "LanguageTool")).toContainText("Turn on");
  await expect(row(page, "Readability")).toContainText("Turn on");
  await expect(page.locator("iframe[data-plugin]")).toHaveCount(0);
});

test("LanguageTool asks before sending text, then proposes fixes signed as itself", async ({ page }) => {
  const sent = [];
  await page.route("https://api.languagetool.org/v2/check", async (route) => {
    const params = new URLSearchParams(route.request().postData() ?? "");
    const text = params.get("text") ?? "";
    sent.push({ text, language: params.get("language") });
    const word = /\p{L}{3,}/u.exec(text);
    await route.fulfill({
      contentType: "application/json",
      headers: { "access-control-allow-origin": "*" },
      body: JSON.stringify({ matches: word ? [{ offset: word.index, length: word[0].length, message: "Possible typo", replacements: [{ value: word[0].toUpperCase() }], rule: { id: "MOCK", issueType: "misspelling", category: { name: "Typos" } } }] : [] }),
    });
  });
  await gotoEditor(page);
  await openManager(page);
  await row(page, "LanguageTool").locator("button", { hasText: "Turn on" }).click();
  // The grant prompt names the server text will go to.
  await expect(page.locator("#pluginGrantDialog")).toBeVisible();
  await expect(page.locator("#pluginGrantList li")).toContainText(["Read this document", "Propose changes as suggestions", "Connect to: api.languagetool.org"]);
  await page.locator("#pluginGrantAllow").click();
  await page.locator("#pluginsDone").click();
  await page.keyboard.press("Escape");

  await runFromAddins(page, "Check with LanguageTool");
  await expect(page.locator("#pluginPanel")).toBeVisible();
  const frame = page.frameLocator('iframe[data-plugin="org.casualoffice.languagetool"]');
  await expect(frame.locator("#disclosure")).toHaveText("Text is checked by api.languagetool.org.");
  await expect(frame.locator(".pp-item").first()).toBeVisible();
  expect(sent.length).toBeGreaterThan(0);
  await frame.locator(".pp-item .pp-item-actions .btn").first().click();
  await expect(page.locator("#aiPreviewPanel")).toBeVisible();
  await expect(page.locator("#aiPreviewMode")).toContainText("LanguageTool (add-in)");
  await page.locator("#aiPreviewPanel .ai-preview-accept").click();
  await expect.poll(async () => page.evaluate(async () => (await window.opendoc.ai.read({ tool: "trackedChanges" })).value.trackedChanges.map((c) => c.author))).toContain("LanguageTool (add-in)");
});

test("Readability runs on the device and proposes comments signed as itself", async ({ page }) => {
  const requested = [];
  page.on("request", (request) => requested.push(request.url()));
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("This is very simple.");
  await openManager(page);
  await row(page, "Readability").locator("button", { hasText: "Turn on" }).click();
  // Installed with the product and sending nothing anywhere: no prompt.
  await expect(page.locator("#pluginGrantDialog")).toBeHidden();
  await page.locator("#pluginsDone").click();
  await page.keyboard.press("Escape");
  await runFromAddins(page, "Check readability");
  const frame = page.frameLocator('iframe[data-plugin="org.casualoffice.readability"]');
  await expect(frame.locator("#score")).toContainText("Reading ease");
  const filler = frame.locator(".pp-item", { hasText: "Filler word" }).first();
  await expect(filler).toBeVisible();
  await filler.locator(".btn", { hasText: "Add comment" }).click();
  await expect(page.locator("#aiPreviewMode")).toContainText("Readability (add-in)");
  const origin = new URL(page.url()).origin;
  expect(requested.filter((u) => /^https?:/.test(u) && new URL(u).origin !== origin)).toEqual([]);
});
